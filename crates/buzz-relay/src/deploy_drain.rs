//! Per-boot deployment admission and transitive work accounting.
//!
//! HTTP stays admitted until the controller confirms the new route and fences
//! this boot. Existing accepted parents
//! retain the right to register descendants until their own work completes.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::task::{Context, Poll};

use axum::{
    body::{Body, Bytes},
    extract::{rejection::JsonRejection, DefaultBodyLimit, State},
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use http_body::Body as HttpBody;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Monotonic deployment lifecycle for one relay boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DrainPhase {
    /// Normal operation; all root admissions are open.
    Serving,
    /// Autonomous admissions closed; root HTTP remains open until fenced.
    Quiescing,
    /// Root HTTP fenced; accepted descendants may finish.
    DrainingWork,
    /// All admission barriers and tracked completion conditions hold.
    SafeToStop,
}

/// Lifetime whose completion is required before process termination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkKind {
    /// Full HTTP handler and streaming body lifetime.
    Http,
    /// Git process, streams and final durable ref publication.
    Git,
    /// Media body and complete object write.
    Upload,
    /// Accepted WebSocket connection.
    WebSocket,
    /// Background job, batch or post-commit side effect.
    Background,
    /// Authenticated callback descended from a live accepted parent.
    Callback,
}

impl WorkKind {
    fn unresolved(self) -> &'static str {
        match self {
            Self::Http => "unresolved-http",
            Self::Git => "unresolved-git",
            Self::Upload => "unresolved-upload",
            Self::WebSocket => "unresolved-websocket",
            Self::Background => "unresolved-background",
            Self::Callback => "unresolved-callback",
        }
    }
}

/// Atomic observation of admission and outstanding work for one boot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DrainSnapshot {
    /// Process-lifetime identity; a restart gets a new UUID.
    pub boot_id: Uuid,
    /// The single operation that began this irreversible drain.
    pub operation_id: Option<Uuid>,
    /// Current lifecycle phase.
    pub phase: DrainPhase,
    /// Full HTTP lifetimes still owned by this relay.
    pub http_inflight: u64,
    /// Git lifetimes still owned by this relay.
    pub git_inflight: u64,
    /// Media writes still owned by this relay.
    pub upload_inflight: u64,
    /// Accepted WebSocket lifetimes still open.
    pub ws_open: u64,
    /// Accepted jobs and side effects still outstanding.
    pub background_inflight: u64,
    /// Authenticated descendant callbacks still outstanding.
    pub callback_inflight: u64,
    /// Whether autonomous root work has been fenced.
    pub autonomous_admissions_stopped: bool,
    /// Whether this relay still admits new root HTTP operations.
    pub root_http_open: bool,
    /// Durable proof reference supplied by the trusted deployment controller.
    pub routing_evidence_ref: Option<String>,
    /// True only after both admission barriers and all completion conditions.
    pub safe_to_stop: bool,
    /// Bounded diagnostic labels for unresolved work; never payloads or secrets.
    pub blocking_workers: Vec<String>,
}

/// An admission, operation or routing-evidence precondition was not met.
#[derive(Debug, PartialEq, Eq)]
pub struct DrainError;

impl std::fmt::Display for DrainError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("deployment admission or fence precondition was not met")
    }
}

impl std::error::Error for DrainError {}

struct Inner {
    boot_id: Uuid,
    operation_id: Option<Uuid>,
    phase: DrainPhase,
    routing_evidence: Option<String>,
    work: HashMap<Uuid, WorkKind>,
    workers: BTreeMap<&'static str, bool>,
    faults: BTreeSet<&'static str>,
}

#[derive(Clone)]
pub(crate) struct ParentContext {
    inner: Weak<Mutex<Inner>>,
    id: Uuid,
}

tokio::task_local! {
    static CURRENT_PARENT: ParentContext;
}

impl Inner {
    fn snapshot(&self) -> DrainSnapshot {
        let count = |kind| self.work.values().filter(|value| **value == kind).count() as u64;
        let safe = self.phase == DrainPhase::DrainingWork
            && self.routing_evidence.is_some()
            && self.work.is_empty()
            && self.workers.values().all(|quiesced| *quiesced)
            && self.faults.is_empty();
        let blockers: BTreeSet<_> = self
            .faults
            .iter()
            .copied()
            .chain(
                self.workers
                    .iter()
                    .filter_map(|(name, quiesced)| (!quiesced).then_some(*name)),
            )
            .collect();
        DrainSnapshot {
            boot_id: self.boot_id,
            operation_id: self.operation_id,
            phase: if safe {
                DrainPhase::SafeToStop
            } else {
                self.phase
            },
            http_inflight: count(WorkKind::Http),
            git_inflight: count(WorkKind::Git),
            upload_inflight: count(WorkKind::Upload),
            ws_open: count(WorkKind::WebSocket),
            background_inflight: count(WorkKind::Background),
            callback_inflight: count(WorkKind::Callback),
            autonomous_admissions_stopped: self.phase != DrainPhase::Serving,
            root_http_open: matches!(self.phase, DrainPhase::Serving | DrainPhase::Quiescing),
            routing_evidence_ref: self.routing_evidence.clone(),
            safe_to_stop: safe,
            blocking_workers: blockers.into_iter().map(str::to_owned).collect(),
        }
    }
}

/// Shared admission barrier for one relay boot. Clones share the same lock.
#[derive(Clone)]
pub struct DrainController {
    inner: Arc<Mutex<Inner>>,
    quiescing: tokio_util::sync::CancellationToken,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    match inner.lock() {
        Ok(state) => state,
        Err(poisoned) => {
            let mut state = poisoned.into_inner();
            state.faults.insert("controller-poisoned");
            state
        }
    }
}

impl Default for DrainController {
    fn default() -> Self {
        Self::new()
    }
}

impl DrainController {
    /// Create an independent serving boot. Runtime worker registration is wired
    /// by the application before exposing deployment control.
    pub fn new() -> Self {
        Self::with_workers(&[])
    }

    pub(crate) fn with_workers(names: &[&'static str]) -> Self {
        Self {
            quiescing: tokio_util::sync::CancellationToken::new(),
            inner: Arc::new(Mutex::new(Inner {
                boot_id: Uuid::new_v4(),
                operation_id: None,
                phase: DrainPhase::Serving,
                routing_evidence: None,
                work: HashMap::new(),
                workers: names.iter().map(|name| (*name, false)).collect(),
                faults: BTreeSet::new(),
            })),
        }
    }

    /// Observe admission and work atomically with descendant registration.
    pub fn snapshot(&self) -> DrainSnapshot {
        let snapshot = lock(&self.inner).snapshot();
        metrics::gauge!("buzz_deploy_root_http_open").set(f64::from(snapshot.root_http_open));
        metrics::gauge!("buzz_deploy_safe_to_stop").set(f64::from(snapshot.safe_to_stop));
        metrics::gauge!("buzz_deploy_blocking_workers").set(snapshot.blocking_workers.len() as f64);
        for (kind, count) in [
            ("http", snapshot.http_inflight),
            ("git", snapshot.git_inflight),
            ("upload", snapshot.upload_inflight),
            ("websocket", snapshot.ws_open),
            ("background", snapshot.background_inflight),
            ("callback", snapshot.callback_inflight),
        ] {
            metrics::gauge!("buzz_deploy_inflight", "kind" => kind).set(count as f64);
        }
        for (name, phase) in [
            ("serving", DrainPhase::Serving),
            ("quiescing", DrainPhase::Quiescing),
            ("draining-work", DrainPhase::DrainingWork),
            ("safe-to-stop", DrainPhase::SafeToStop),
        ] {
            metrics::gauge!("buzz_deploy_phase", "phase" => name)
                .set(f64::from(snapshot.phase == phase));
        }
        snapshot
    }

    pub(crate) fn ready_to_join_runtime(&self) -> bool {
        let state = lock(&self.inner);
        state.phase == DrainPhase::DrainingWork
            && state.routing_evidence.is_some()
            && state.work.is_empty()
            && state.faults.is_empty()
            && state
                .workers
                .iter()
                .all(|(name, stopped)| *name == crate::deploy_runtime::WORKER || *stopped)
    }

    /// Seal the qualified worker census before exposing listeners. Missing or
    /// unexpected workers leave the bootstrap barrier in place.
    pub(crate) fn seal_worker_inventory(&self, expected: &[&str]) -> Result<(), DrainError> {
        let mut state = lock(&self.inner);
        let actual: BTreeSet<_> = state
            .workers
            .keys()
            .copied()
            .filter(|name| *name != "worker-inventory-incomplete")
            .collect();
        let expected: BTreeSet<_> = expected.iter().copied().collect();
        if state.phase != DrainPhase::Serving
            || !state.workers.contains_key("worker-inventory-incomplete")
            || actual != expected
        {
            return Err(DrainError);
        }
        state.workers.remove("worker-inventory-incomplete");
        Ok(())
    }

    /// Register a worker before it starts. A late birth invalidates drain proof.
    pub fn register_worker(&self, name: &'static str) -> Result<(), DrainError> {
        let mut state = lock(&self.inner);
        if state.phase != DrainPhase::Serving {
            state.faults.insert("late-worker");
            return Err(DrainError);
        }
        if state.workers.contains_key(name) {
            return Err(DrainError);
        }
        state.workers.insert(name, false);
        Ok(())
    }

    /// Acknowledge a joined/fenced worker after autonomous admission has closed.
    /// This never clears unresolved-effect faults left by dropped work.
    pub fn worker_quiesced(&self, name: &str) -> Result<(), DrainError> {
        let mut state = lock(&self.inner);
        if state.phase == DrainPhase::Serving {
            return Err(DrainError);
        }
        let worker = state.workers.get_mut(name).ok_or(DrainError)?;
        *worker = true;
        Ok(())
    }

    /// Close autonomous admissions once; a conflicting operation is rejected.
    pub fn begin(&self, operation_id: Uuid) -> Result<DrainSnapshot, DrainError> {
        self.begin_inner(operation_id, false)
            .map(|(snapshot, _)| snapshot)
    }

    fn begin_inner(
        &self,
        operation_id: Uuid,
        own_close_work: bool,
    ) -> Result<(DrainSnapshot, Option<WorkGuard>), DrainError> {
        let mut state = lock(&self.inner);
        if state
            .operation_id
            .is_some_and(|current| current != operation_id)
        {
            return Err(DrainError);
        }
        let first = state.phase == DrainPhase::Serving;
        if first {
            state.operation_id = Some(operation_id);
            state.phase = DrainPhase::Quiescing;
            self.quiescing.cancel();
        }
        let work =
            (first && own_close_work).then(|| self.insert_work(&mut state, WorkKind::Background));
        Ok((state.snapshot(), work))
    }

    /// Close HTTP roots after the trusted controller confirms the new route.
    /// Bind its evidence to this operation and boot. Requests still buffered
    /// outside this relay are not admitted work and may require client retry.
    pub fn fence_http(
        &self,
        operation_id: Uuid,
        boot_id: Uuid,
        routing_evidence_ref: &str,
    ) -> Result<DrainSnapshot, DrainError> {
        let mut state = lock(&self.inner);
        if state.operation_id != Some(operation_id)
            || state.boot_id != boot_id
            || state.phase == DrainPhase::Serving
            || routing_evidence_ref.is_empty()
            || routing_evidence_ref.len() > 512
            || !routing_evidence_ref
                .bytes()
                .all(|byte| (0x21..=0x7e).contains(&byte))
            || state
                .routing_evidence
                .as_deref()
                .is_some_and(|current| current != routing_evidence_ref)
        {
            return Err(DrainError);
        }
        state.routing_evidence = Some(routing_evidence_ref.to_owned());
        state.phase = DrainPhase::DrainingWork;
        Ok(state.snapshot())
    }

    /// Admit root HTTP before the local fence, or autonomous work while
    /// serving. Callbacks always require a live accepted parent.
    pub fn try_admit_root(&self, kind: WorkKind) -> Result<WorkGuard, DrainError> {
        let mut state = lock(&self.inner);
        let admitted = match kind {
            WorkKind::Background => state.phase == DrainPhase::Serving,
            WorkKind::Callback => false,
            _ => matches!(state.phase, DrainPhase::Serving | DrainPhase::Quiescing),
        };
        if !admitted {
            return Err(DrainError);
        }
        Ok(self.insert_work(&mut state, kind))
    }

    /// Register a descendant before releasing its accepted parent, even after
    /// root admission is closed. Foreign or already-completed parents fail.
    pub fn admit_child(&self, parent: &WorkGuard, kind: WorkKind) -> Result<WorkGuard, DrainError> {
        if !Arc::ptr_eq(&self.inner, &parent.inner) || parent.completed {
            return Err(DrainError);
        }
        let mut state = lock(&self.inner);
        if !state.work.contains_key(&parent.id) {
            return Err(DrainError);
        }
        Ok(self.insert_work(&mut state, kind))
    }

    /// Admit a child from the current explicitly scoped accepted future. An
    /// unrelated task has no inherited capability and cannot become a root.
    pub fn admit_current_child(&self, kind: WorkKind) -> Result<WorkGuard, DrainError> {
        match CURRENT_PARENT.try_with(|parent| self.admit_parent_context(parent, kind)) {
            Ok(result) => result,
            Err(_) => Err(DrainError),
        }
    }

    pub(crate) fn admit_parent_context(
        &self,
        parent: &ParentContext,
        kind: WorkKind,
    ) -> Result<WorkGuard, DrainError> {
        let owner = parent.inner.upgrade().ok_or(DrainError)?;
        if !Arc::ptr_eq(&owner, &self.inner) {
            return Err(DrainError);
        }
        let mut state = lock(&self.inner);
        if !state.work.contains_key(&parent.id) {
            return Err(DrainError);
        }
        Ok(self.insert_work(&mut state, kind))
    }

    pub(crate) fn record_fault(&self, label: &'static str) {
        lock(&self.inner).faults.insert(label);
    }

    /// Retain an already accepted effect even if its caller lost lineage.
    /// Missing attribution permanently blocks termination; this is never an
    /// admission path for a new autonomous job or incoming request.
    pub(crate) fn track_accepted_child(&self, kind: WorkKind) -> WorkGuard {
        match self.admit_current_child(kind) {
            Ok(work) => work,
            Err(_) => {
                let mut state = lock(&self.inner);
                state.faults.insert("unattributed-work");
                self.insert_work(&mut state, kind)
            }
        }
    }

    fn insert_work(&self, state: &mut Inner, kind: WorkKind) -> WorkGuard {
        let id = Uuid::new_v4();
        state.work.insert(id, kind);
        WorkGuard {
            inner: Arc::clone(&self.inner),
            id,
            completed: false,
        }
    }
}

impl buzz_workflow::lifecycle::RunAdmission for DrainController {
    fn admit(
        &self,
        autonomous: bool,
    ) -> Result<
        Box<dyn buzz_workflow::lifecycle::RunLease>,
        buzz_workflow::lifecycle::AdmissionClosed,
    > {
        let work = if autonomous {
            self.try_admit_root(WorkKind::Background)
                .map_err(|_| buzz_workflow::lifecycle::AdmissionClosed)?
        } else {
            self.track_accepted_child(WorkKind::Background)
        };
        Ok(Box::new(work))
    }
}

impl buzz_workflow::lifecycle::RunLease for WorkGuard {
    fn with_context(&self, callback: &mut dyn FnMut()) {
        WorkGuard::with_context(self, callback);
    }

    fn complete(self: Box<Self>) {
        (*self).complete();
    }
}

/// Ownership of one complete work lifetime. Dropping without explicit
/// completion records unresolved work, including task abort and panic paths.
#[must_use = "accepted work must retain its guard until completion"]
pub struct WorkGuard {
    inner: Arc<Mutex<Inner>>,
    id: Uuid,
    completed: bool,
}

impl WorkGuard {
    pub(crate) fn parent_context(&self) -> ParentContext {
        ParentContext {
            inner: Arc::downgrade(&self.inner),
            id: self.id,
        }
    }

    pub(crate) fn with_context<T>(&self, action: impl FnOnce() -> T) -> T {
        CURRENT_PARENT.sync_scope(self.parent_context(), action)
    }

    /// Run a future with this live work as its parent, preserving context over
    /// suspension without granting it to unrelated spawned tasks. Completion
    /// remains explicit; scope does not acknowledge effects on the caller's behalf.
    pub async fn scope<F: Future>(&self, future: F) -> F::Output {
        CURRENT_PARENT.scope(self.parent_context(), future).await
    }

    /// Acknowledge known completion only after all owned effects have ended or
    /// a separately proven durable recovery/fencing contract has taken over.
    pub fn complete(mut self) {
        self.completed = true;
    }
}

impl Drop for WorkGuard {
    fn drop(&mut self) {
        let mut state = lock(&self.inner);
        if let Some(kind) = state.work.remove(&self.id) {
            if !self.completed || std::thread::panicking() {
                state.faults.insert(kind.unresolved());
            }
        }
    }
}

/// Complete accepted work only after its future returns normally. Cancellation
/// and panic drop the unfinished guard and retain an unresolved-work fault.
pub(crate) async fn run_accepted<F: Future>(work: Option<WorkGuard>, future: F) -> F::Output {
    if let Some(work) = work {
        let result = work.scope(future).await;
        work.complete();
        result
    } else {
        future.await
    }
}

/// Registered autonomous worker. Construct before spawning and acknowledge
/// only after its admitted batch and owned resources have completed.
pub struct Worker {
    drain: Option<DrainController>,
    name: &'static str,
}

impl Worker {
    /// Register synchronously; legacy workers have no deployment admission.
    pub fn new(drain: Option<DrainController>, name: &'static str) -> Self {
        if let Some(drain) = &drain {
            if drain.register_worker(name).is_err() {
                drain.record_fault("worker-registration-failed");
            }
        }
        Self { drain, name }
    }

    /// Admit before polling any effect and preserve context through the entire
    /// batch. An error, panic or cancellation retains an unresolved-work fault.
    pub async fn batch<T, E>(
        &self,
        future: impl Future<Output = Result<T, E>>,
    ) -> Option<Result<T, E>> {
        if let Some(drain) = &self.drain {
            let work = drain.try_admit_root(WorkKind::Background).ok()?;
            let result = work.scope(future).await;
            if result.is_ok() {
                work.complete();
            }
            Some(result)
        } else {
            Some(future.await)
        }
    }

    /// Interrupt an idle wait at begin-drain. Never wrap side effects in this
    /// method: admitted batches must finish through `batch` without cancellation.
    pub async fn idle<T>(&self, wait: impl Future<Output = T>) -> Option<T> {
        if let Some(drain) = &self.drain {
            tokio::select! {
                biased;
                _ = drain.quiescing.cancelled() => None,
                result = wait => Some(result),
            }
        } else {
            Some(wait.await)
        }
    }

    /// Acknowledge that the loop has exited and released its owned resources.
    pub fn finish(self) {
        if let Some(drain) = &self.drain {
            if drain.worker_quiesced(self.name).is_err() {
                drain.record_fault("worker-quiescence-failed");
            }
        }
    }
}

/// Preserve the live parent across rescheduling of an already owned future.
/// The caller retains and joins that future before releasing its parent guard.
pub(crate) fn preserve_context<F: Future>(future: F) -> impl Future<Output = F::Output> {
    let parent = CURRENT_PARENT.try_with(Clone::clone).ok();
    async move {
        match parent {
            Some(parent) => CURRENT_PARENT.scope(parent, future).await,
            None => future.await,
        }
    }
}

/// Complete an accepted effect only after its result is known to be successful.
pub(crate) async fn run_accepted_result<T, E>(
    work: Option<WorkGuard>,
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, E> {
    if let Some(work) = work {
        let result = work.scope(future).await;
        if result.is_ok() {
            work.complete();
        }
        result
    } else {
        future.await
    }
}

/// Register a periodic worker before spawning it. Fence each batch before its
/// first effect, finish an admitted batch without cancellation, and wake idle
/// workers at begin-drain so their completion does not depend on the period.
/// A failed batch remains unresolved; callers must return an error for unknown
/// outcomes, and may log its details themselves. Legacy mode retains the loop.
pub fn periodic_worker<F, Fut, E>(
    drain: Option<DrainController>,
    name: &'static str,
    period: std::time::Duration,
    mut tick: F,
) -> impl Future<Output = ()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    if let Some(drain) = &drain {
        if drain.register_worker(name).is_err() {
            drain.record_fault("worker-registration-failed");
        }
    }
    async move {
        loop {
            if let Some(drain) = &drain {
                let Ok(work) = drain.try_admit_root(WorkKind::Background) else {
                    break;
                };
                if work.scope(tick()).await.is_ok() {
                    work.complete();
                }
                tokio::select! {
                    _ = drain.quiescing.cancelled() => break,
                    _ = tokio::time::sleep(period) => {}
                }
            } else {
                let _ = tick().await;
                tokio::time::sleep(period).await;
            }
        }
        if let Some(drain) = &drain {
            if drain.worker_quiesced(name).is_err() {
                drain.record_fault("worker-quiescence-failed");
            }
        }
    }
}

/// An upgrade either transfers ownership to its connection or fails before
/// any socket work starts. The shared slot is never locked across an await.
#[derive(Clone)]
pub(crate) struct UpgradeWork(Arc<Mutex<Option<WorkGuard>>>);

impl UpgradeWork {
    pub(crate) fn new(work: Option<WorkGuard>) -> Self {
        Self(Arc::new(Mutex::new(work)))
    }

    pub(crate) fn failed(self) {
        if let Some(work) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            work.complete();
        }
    }

    pub(crate) async fn run<F: Future>(self, future: F) -> F::Output {
        let work = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        run_accepted(work, future).await
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BeginRequest {
    operation_id: Uuid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FenceRequest {
    operation_id: Uuid,
    boot_id: Uuid,
    routing_evidence_ref: String,
}

pub(crate) fn control_router(state: Arc<crate::state::AppState>) -> Router {
    Router::new()
        .route("/_deploy/drain", get(control_snapshot).post(control_begin))
        .route("/_deploy/drain/fence-http", post(control_fence))
        .layer(DefaultBodyLimit::max(4096))
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            authenticate,
        ))
        .with_state(state)
}

pub(crate) async fn track_http(
    State(state): State<Arc<crate::state::AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if state.config.deploy_control_token.is_none() {
        return next.run(request).await;
    }
    let (request, work) = if request.uri().path() == "/internal/git/policy" {
        match crate::api::git::deploy::admit_callback_request(&state, request).await {
            Ok(admitted) => admitted,
            Err(response) => return response,
        }
    } else {
        let Ok(work) = state.deploy_drain.try_admit_root(WorkKind::Http) else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        (request, work)
    };
    // An aborted/panicking handler drops unfinished work and records a fault.
    // Once headers exist, the body owns the remaining response lifetime.
    let response = work.scope(next.run(request)).await;
    if response.body().is_end_stream() {
        work.complete();
        return response;
    }
    let (parts, body) = response.into_parts();
    Response::from_parts(
        parts,
        Body::new(TrackedResponseBody {
            body: Box::pin(body),
            work: Some(work),
            failed: false,
        }),
    )
}

struct TrackedResponseBody {
    body: Pin<Box<Body>>,
    work: Option<WorkGuard>,
    failed: bool,
}

impl HttpBody for TrackedResponseBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        let body = &mut this.body;
        let result = match &this.work {
            Some(work) => work.with_context(|| body.as_mut().poll_frame(context)),
            None => body.as_mut().poll_frame(context),
        };
        if matches!(&result, Poll::Ready(Some(Err(_)))) {
            this.failed = true;
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.body.as_ref().get_ref().is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.body.as_ref().get_ref().size_hint()
    }
}

impl Drop for TrackedResponseBody {
    fn drop(&mut self) {
        if let Some(work) = self.work.take() {
            if !self.failed {
                // A normal body drop ends its transport lifetime, including a
                // disconnected reader after headers. Any persistent producer
                // must own a separate child guard until its effects complete.
                work.complete();
            }
        }
    }
}

async fn authenticate(
    State(state): State<Arc<crate::state::AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(token) = &state.config.deploy_control_token else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut credentials = request.headers().get_all(header::AUTHORIZATION).iter();
    let Some(credential) = credentials.next() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if credentials.next().is_some()
        || !credential
            .as_bytes()
            .strip_prefix(b"Bearer ")
            .is_some_and(|provided| token.matches(provided))
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

async fn control_snapshot(State(state): State<Arc<crate::state::AppState>>) -> Json<DrainSnapshot> {
    Json(state.deploy_drain.snapshot())
}

async fn control_begin(
    State(state): State<Arc<crate::state::AppState>>,
    body: Result<Json<BeginRequest>, JsonRejection>,
) -> Response {
    let Ok(Json(request)) = body else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok((snapshot, close_work)) = state.deploy_drain.begin_inner(request.operation_id, true)
    else {
        return StatusCode::CONFLICT.into_response();
    };
    state.begin_shutdown();
    if let Some(close_work) = close_work {
        tokio::spawn(async move {
            close_work
                .scope(async {
                    if state.config.drain_jitter_ms == 0 {
                        state.conn_manager.drain_all();
                    } else {
                        state
                            .conn_manager
                            .drain_all_jittered(state.config.drain_jitter_ms)
                            .await;
                    }
                    state.community_connections.restart_all();
                })
                .await;
            close_work.complete();
        });
    }
    (StatusCode::ACCEPTED, Json(snapshot)).into_response()
}

async fn control_fence(
    State(state): State<Arc<crate::state::AppState>>,
    body: Result<Json<FenceRequest>, JsonRejection>,
) -> Response {
    let Ok(Json(request)) = body else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match state.deploy_drain.fence_http(
        request.operation_id,
        request.boot_id,
        &request.routing_evidence_ref,
    ) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(_) => StatusCode::CONFLICT.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn response_body_descendant_inherits_live_parent_after_http_fence() {
        let controller = DrainController::new();
        let parent = controller
            .try_admit_root(WorkKind::Http)
            .expect("accepted response");
        let producer = controller.clone();
        let (child_tx, child_rx) = tokio::sync::oneshot::channel();
        let stream = futures::stream::once(async move {
            let child = producer
                .admit_current_child(WorkKind::Background)
                .expect("stream producer must inherit its accepted HTTP parent");
            assert!(child_tx.send(child).is_ok());
            Ok::<_, std::convert::Infallible>(Bytes::from_static(b"fixture"))
        });
        let body = TrackedResponseBody {
            body: Box::pin(Body::from_stream(stream)),
            work: Some(parent),
            failed: false,
        };
        let operation = Uuid::new_v4();
        let begun = controller.begin(operation).expect("begin");
        controller
            .fence_http(operation, begun.boot_id, "evidence:retired")
            .expect("fence");
        assert_eq!(
            body.collect().await.expect("body completes").to_bytes(),
            "fixture"
        );
        let child = child_rx.await.expect("descendant registered");
        let snapshot = controller.snapshot();
        assert_eq!(snapshot.http_inflight, 0);
        assert_eq!(snapshot.background_inflight, 1);
        assert!(!snapshot.safe_to_stop);
        child.complete();
        assert!(controller.snapshot().safe_to_stop);
    }
}
