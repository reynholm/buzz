//! Status input is answered from host state without entering or cancelling ACP.
use super::*;
impl NormalListenerIngress {
    pub(super) fn runtime_status(
        &self,
        pool: &AgentPool,
        queue: &EventQueue,
        scope: &scope::SessionScope,
        dm: bool,
    ) -> Option<(String, ThreadTags)> {
        if self.buzz_event.event.kind != nostr::Kind::Custom(9)
            || !matches!(
                self.buzz_event.event.content.trim(),
                "!status" | "status?" | "статус?"
            )
        {
            return None;
        }
        let input = queue::BatchEvent {
            event: self.buzz_event.event.clone(),
            edit: self.edit.clone(),
            received_at: std::time::Instant::now(),
            prompt_tag: self.prompt_tag.clone(),
        };
        let route = input.reply_route();
        let text=pool.provider_background_status(scope,&route,dm).or_else(|| {
            (queue.in_flight_prompt_routing(scope).is_some_and(|routing|routing.provider_background())
                && queue.in_flight_accepts_steer(scope,&route,dm))
                .then(||"Work is running in this conversation. Waiting for the current provider operation.".into())
        })?;
        let mut tags = input.routing_thread_tags();
        if !dm && tags.root_event_id.is_none() {
            tags.root_event_id = Some(input.reply_thread());
            tags.parent_event_id = tags.root_event_id.clone();
        }
        Some((text, tags))
    }
}
