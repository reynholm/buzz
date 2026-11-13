#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
trap 'printf "mobile contract failed at line %s: %s\n" "$LINENO" "$BASH_COMMAND" >&2' ERR
# Policy and DCO App IDs below are synthetic fixture values, not installed
# fork identities. Production policy IDs have no defaults.
export BUZZ_MOBILE_RELEASE_TAG_RULESET_ID=1001
export BUZZ_MOBILE_RELEASE_BRANCH_RULESET_ID=1002
mkdir -p "$tmp/scripts"
cp "$repo_root/scripts/mobile-release.sh" "$repo_root/scripts/mobile-release-source.sh" \
  "$repo_root/scripts/release-rulesets.sh" "$tmp/scripts/"
script="$tmp/scripts/mobile-release.sh"
remote="$tmp/remote.git"
work="$tmp/work"
operator="$tmp/operator"
bin="$tmp/bin"
canonical_origin="git@github.com:reynholm/buzz.git"
mkdir -p "$bin"

cat > "$bin/gh" <<'GH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == --version ]]; then
  printf 'gh version %s (test)\n' "${GH_VERSION:-2.94.0}"
  exit
fi
[[ "${1:-}" == api ]] || exit 2
shift
query='' slurp=false endpoint=''
while [[ $# -gt 0 ]]; do
  case "$1" in
    --paginate) shift ;;
    --slurp) slurp=true; shift ;;
    --jq) query="$2"; shift 2 ;;
    *) [[ -z "$endpoint" ]] || exit 2; endpoint="$1"; shift ;;
  esac
done
case "$endpoint" in
  user) payload=$(jq -n --arg login "${GH_ACTOR-reynholm}" '{login:$login,id:77}') ;;
  repos/reynholm/buzz/rulesets/1002)
    [[ "${GH_BRANCH_POLICY_FAIL:-}" != 1 ]] || exit 1
    policy='{"target":"branch","enforcement":"active","conditions":{"ref_name":{"include":["refs/heads/main","refs/heads/mobile-v*"],"exclude":[]}},"bypass_actors":[],"rules":[{"type":"deletion"},{"type":"non_fast_forward"},{"type":"pull_request","parameters":{"required_approving_review_count":1,"dismiss_stale_reviews_on_push":true,"require_last_push_approval":true,"required_review_thread_resolution":true}},{"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true,"required_status_checks":[{"context":"Mobile","integration_id":15368},{"context":"DCO Check","integration_id":3001}]}}]}'
    payload="${GH_BRANCH_POLICY:-$policy}"
    payload=$(jq --argjson dco "${GH_POLICY_DCO_APP:-3001}" \
      --argjson mobile "${GH_POLICY_MOBILE_APP:-15368}" '
      (.rules[] | select(.type=="required_status_checks") | .parameters.required_status_checks[] | select(.context=="DCO Check") | .integration_id) = $dco |
      (.rules[] | select(.type=="required_status_checks") | .parameters.required_status_checks[] | select(.context=="Mobile") | .integration_id) = $mobile
    ' <<<"$payload")
    if [[ "${GH_POLICY_DCO_ABSENT:-}" == 1 ]]; then
      payload=$(jq '(.rules[] | select(.type=="required_status_checks") | .parameters.required_status_checks[] | select(.context=="DCO Check")) |= del(.integration_id)' <<<"$payload")
    fi
    if [[ "${GH_EXTRA_REQUIRED_CONTEXT:-}" == 1 ]]; then
      payload=$(jq '.rules[] |= if .type=="required_status_checks" then .parameters.required_status_checks += [{context:"Unknown check",integration_id:3001}] else . end' <<<"$payload")
    fi
    ;;
  repos/reynholm/buzz/rulesets/1001)
    payload=$(jq -n --arg state "${GH_TAG_RULESET_STATE:-active}" \
      --arg target "${GH_TAG_RULESET_TARGET:-tag}" --arg bypass "${GH_CURRENT_USER_CAN_BYPASS-always}" \
      --arg types "${GH_TAG_RULE_TYPES:-creation,deletion,non_fast_forward,update}" \
      --arg includes "${GH_TAG_INCLUDES:-refs/tags/mobile-*}" --arg excludes "${GH_TAG_EXCLUDES:-}" \
      --argjson actor "${GH_TAG_ACTOR_ID:-77}" \
      '{target:$target,enforcement:$state,current_user_can_bypass:$bypass,
        bypass_actors:[{actor_type:"User",actor_id:$actor,bypass_mode:"always"}],
        conditions:{ref_name:{include:($includes|split(",")),exclude:(if $excludes=="" then [] else $excludes|split(",") end)}},
        rules:($types|split(",")|map({type:.}))}')
    ;;
  repos/reynholm/buzz/git/ref/tags/*)
    [[ "${GH_TAG_REF_FAIL:-}" != 1 ]] || exit 1
    oid=$(git --git-dir="$GH_REMOTE" rev-parse --verify --quiet "refs/tags/${endpoint#repos/reynholm/buzz/git/ref/tags/}") || exit 1
    payload=$(jq -n --arg oid "$oid" '{object:{sha:$oid}}')
    ;;
  repos/reynholm/buzz/compare/*)
    [[ "${GH_COMPARE_FAIL:-}" != 1 ]] || exit 1
    pair="${endpoint##*/}"
    oid=$(git --git-dir="$GH_REMOTE" merge-base "${pair%%...*}" "${pair##*...}")
    payload=$(jq -n --arg oid "$oid" '{merge_base_commit:{sha:$oid}}')
    ;;
  repos/reynholm/buzz/commits/*/check-runs\?*)
    sha="${endpoint#repos/reynholm/buzz/commits/}"; sha="${sha%%/*}"
    printf '%s\n' "$sha" > "$GH_CHECK_REQUEST_FILE"
    payload=$(jq -n --arg sha "${GH_CHECK_SHA:-$sha}" --arg status "${GH_CHECK_STATUS:-completed}" \
      --arg conclusion "${GH_CHECK_CONCLUSION:-success}" --argjson app "${GH_CHECK_APP:-15368}" \
      --argjson dco "${GH_DCO_CHECK_APP:-3001}" \
      '{check_runs:[{id:100,name:"Mobile",head_sha:$sha,app:{id:$app},check_suite:{id:12},status:$status,conclusion:$conclusion,
         details_url:"https://github.com/reynholm/buzz/actions/runs/99/job/100"},
        {id:101,name:"DCO Check",head_sha:$sha,app:{id:$dco},status:"completed",conclusion:"success"}]}')
    if [[ "${GH_EXTRA_REQUIRED_CONTEXT:-}" == 1 ]]; then
      payload=$(jq --arg sha "$sha" '.check_runs += [{id:104,name:"Unknown check",head_sha:$sha,app:{id:3001},status:"completed",conclusion:"success"}]' <<<"$payload")
    fi
    if [[ "${GH_DCO_CHECK_MISSING:-}" == 1 ]]; then
      payload=$(jq 'del(.check_runs[1])' <<<"$payload")
    fi
    if [[ "${GH_DCO_NEWER_FAILURE:-}" == 1 ]]; then
      payload=$(jq '.check_runs += [(.check_runs[1] | .id=103 | .conclusion="failure")]' <<<"$payload")
    fi
    if [[ "${GH_CHECK_MISSING:-}" == 1 ]]; then payload='{"check_runs":[]}'; fi
    if [[ -n "${GH_ONLY_QUALIFIED_SHA:-}" && "$sha" != "$GH_ONLY_QUALIFIED_SHA" ]]; then
      payload='{"check_runs":[]}'
    fi
    if [[ "${GH_CHECK_NEWER_FAILURE:-}" == 1 ]]; then
      payload=$(jq '.check_runs += [(.check_runs[0] | .id=102 | .conclusion="failure")]' <<<"$payload")
    fi
    ;;
  repos/reynholm/buzz/actions/workflows/ci.yml) payload='{"id":55,"path":".github/workflows/ci.yml"}' ;;
  repos/reynholm/buzz/actions/runs/99)
    sha="${GH_RUN_SHA:-$(cat "$GH_CHECK_REQUEST_FILE")}"
    payload=$(jq -n --arg sha "$sha" --arg repo "${GH_RUN_REPO:-reynholm/buzz}" --arg path "${GH_RUN_PATH:-.github/workflows/ci.yml}" \
      '{id:99,workflow_id:55,check_suite_id:12,head_sha:$sha,head_repository:{full_name:$repo},event:"push",path:$path,status:"completed",conclusion:"success"}')
    ;;
  *) echo "unexpected API endpoint: $endpoint" >&2; exit 2 ;;
esac
if [[ "$slurp" == true ]]; then payload="[$payload]"; fi
if [[ -n "$query" ]]; then jq -r "$query" <<<"$payload"; else printf '%s\n' "$payload"; fi
GH
chmod +x "$bin/gh"

# A git wrapper lets one case simulate a push that lands but reports failure.
real_git="$(command -v git)"
cat > "$bin/git" <<GIT
#!/usr/bin/env bash
# Replace only the network transport boundary; validation still sees actual
# configured/effective URLs. No fixture command can contact GitHub.
if [[ " \$* " == *" fetch "* || " \$* " == *" push "* || " \$* " == *" clone "* || " \$* " == *" ls-remote "* ]]; then
  transport=(-c "url.file://\$GH_REMOTE.insteadOf=git@github.com:reynholm/buzz.git")
else
  transport=()
fi
if [[ "\${GIT_PUSH_LANDS_BUT_FAILS:-}" == 1 && " \$* " == *" push "* ]]; then
  "$real_git" "\${transport[@]}" "\$@"
  exit 1
fi
# Simulates another fetch of main finishing right after the release script's
# own fetch, replacing the checkout's shared FETCH_HEAD.
if [[ "\${GIT_RACING_MAIN_FETCH:-}" == 1 && " \$* " == *" fetch "* ]]; then
  "$real_git" "\${transport[@]}" "\$@" || exit
  exec "$real_git" "\${transport[@]}" fetch -q --no-tags origin refs/heads/main
fi
exec "$real_git" "\${transport[@]}" "\$@"
GIT
chmod +x "$bin/git"

export PATH="$bin:$PATH"
export GH_REMOTE="$remote"
export GH_CHECK_REQUEST_FILE="$tmp/check-source"

run_release() {
  local repo="$1"
  shift
  (
    cd "$repo"
    git config protocol.file.allow always
    "$script" "$@"
  )
}

fail() {
  echo "$*" >&2
  exit 1
}

assert_no_removed_mobile_release_behavior() {
  local status

  grep -Eq 'gh[[:space:]]+release|finalize' "$@" && \
    fail "removed branch/finalization/GitHub Release behavior remains"
  status="$?"
  [[ "$status" -eq 1 ]] || \
    fail "could not scan for removed branch/finalization/GitHub Release behavior"
}

git init -q --bare "$remote"
git init -q "$work"
git -C "$work" config user.name test
git -C "$work" config user.email test@example.com
git -C "$work" remote add origin "$canonical_origin"
git -C "$work" config protocol.file.allow always
echo first > "$work/file"
git -C "$work" add file
git -C "$work" commit -qm first
git -C "$work" branch -M main
git --git-dir="$remote" symbolic-ref HEAD refs/heads/main
git -C "$work" push -q -u origin main

# Candidate publication must work from a stale operator clone, warn about the
# stale checkout, and target the exact current remote main commit.
git -c "url.file://$remote.insteadOf=$canonical_origin" \
  -c protocol.file.allow=always clone -q "$canonical_origin" "$operator"
git -C "$operator" config user.name test
git -C "$operator" config user.email test@example.com
git -C "$operator" config tag.gpgSign false
echo remote-only >> "$work/file"
git -C "$work" commit -qam remote-only
git -C "$work" push -q origin main
remote_main_sha="$(git --git-dir="$remote" rev-parse refs/heads/main)"
if git -C "$operator" cat-file -e "$remote_main_sha^{commit}" 2>/dev/null; then
  fail "stale-clone fixture already contains the remote-only commit"
fi
run_release "$operator" candidate 1.2.3 > "$tmp/stale-output" 2> "$tmp/stale-error" || {
  cat "$tmp/stale-error" >&2
  fail "qualified exact fork source was rejected"
}
grep -Fq "Note: local HEAD is $(git -C "$operator" rev-parse HEAD); candidate source is pinned from origin/main $remote_main_sha." \
  "$tmp/stale-error"
cat "$tmp/stale-output"
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v1.2.3-rc.1^{commit}')" == \
   "$remote_main_sha" ]]
[[ "$(git --git-dir="$remote" cat-file -t refs/tags/mobile-v1.2.3-rc.1)" == tag ]]
# The operator pushed the tag itself: it exists locally too, annotated.
[[ "$(git -C "$operator" cat-file -t refs/tags/mobile-v1.2.3-rc.1)" == tag ]]

# A stable branch may diverge from main, including an empty release-only commit.
git -C "$work" switch -q -c mobile-v1.2
git -C "$work" commit -q --allow-empty -m stable-only
git -C "$work" push -q origin mobile-v1.2
stable_sha="$(git -C "$work" rev-parse HEAD)"
git -C "$work" switch -q main
if GH_BRANCH_POLICY_FAIL=1 run_release "$operator" candidate 1.2.4 --source-branch mobile-v1.2 > "$tmp/policy-error" 2>&1; then
  fail "operator accepted unavailable stable-branch policy"
fi
grep -Fq 'could not verify configured mobile branch protection' "$tmp/policy-error"
if GH_BRANCH_POLICY='{}' run_release "$operator" candidate 1.2.4 --source-branch mobile-v1.2 > "$tmp/policy-error" 2>&1; then
  fail "operator accepted missing stable-branch policy"
fi
stable_only_policy='{"target":"branch","enforcement":"active","conditions":{"ref_name":{"include":["refs/heads/mobile-v*"],"exclude":[]}},"bypass_actors":[],"rules":[{"type":"deletion"},{"type":"non_fast_forward"},{"type":"pull_request","parameters":{"required_approving_review_count":1,"dismiss_stale_reviews_on_push":true,"require_last_push_approval":true,"required_review_thread_resolution":true}},{"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true,"required_status_checks":[{"context":"Mobile","integration_id":15368},{"context":"DCO Check","integration_id":3001}]}}]}'
if GH_BRANCH_POLICY="$stable_only_policy" run_release "$operator" candidate 1.2.4 --source-branch mobile-v1.2 > "$tmp/policy-error" 2>&1; then
  fail "stable candidate accepted a policy that leaves main unprotected"
fi
[[ -z "$(git -C "$operator" tag -l 'mobile-v1.2.4-rc.*')" ]]
[[ -z "$(git --git-dir="$remote" tag -l 'mobile-v1.2.4-rc.*')" ]]
run_release "$operator" candidate 1.2.4 --source-branch mobile-v1.2
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v1.2.4-rc.1^{commit}')" == "$stable_sha" ]]
# A concurrent fetch of main in the same checkout cannot swap the source.
GIT_RACING_MAIN_FETCH=1 run_release "$operator" candidate 1.2.4 --source-branch mobile-v1.2 >/dev/null
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v1.2.4-rc.2^{commit}')" == "$stable_sha" ]] || \
  fail "a concurrent fetch of main changed the stable candidate's source"
# Existing mismatched refs must be rejected by the guard, not missing-ref errors.
for source in mobile-v1.3 mobile-v2.2 mobile-v01.2 arbitrary; do
  git -C "$work" switch -q -c "$source"
  git -C "$work" commit -q --allow-empty -m "$source"
  git -C "$work" push -q origin "$source"
done
git -C "$work" switch -q main
for source in mobile-v1.3 mobile-v2.2 mobile-v01.2 arbitrary ''; do
  if run_release "$operator" candidate 1.2.4 --source-branch "$source" >/dev/null 2>&1; then
    fail "accepted mismatched source '$source'"
  fi
done
if run_release "$operator" candidate 8.8.0 --source-branch mobile-v8.8 >/dev/null 2>&1; then
  fail "missing stable branch fell back to main"
fi

# Existing remote identities remain unchanged. Later candidates sequence
# monotonically and target the then-current remote main commit.
rc1_tag_oid="$(git --git-dir="$remote" rev-parse refs/tags/mobile-v1.2.3-rc.1)"
echo newer >> "$work/file"
git -C "$work" commit -qam newer
git -C "$work" push -q origin main
new_main_sha="$(git --git-dir="$remote" rev-parse refs/heads/main)"
run_release "$operator" candidate 1.2.3
[[ "$(git --git-dir="$remote" rev-parse refs/tags/mobile-v1.2.3-rc.1)" == "$rc1_tag_oid" ]]
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v1.2.3-rc.2^{commit}')" == \
   "$new_main_sha" ]]
[[ "$(git --git-dir="$remote" cat-file -t refs/tags/mobile-v1.2.3-rc.2)" == tag ]]

# Sequence from the highest exact remote RC even if there are gaps, and ignore
# malformed or other-version tags.
git -C "$work" -c tag.gpgSign=false tag -a -m gap mobile-v1.2.3-rc.7 "$new_main_sha"
git -C "$work" -c tag.gpgSign=false tag -a -m malformed mobile-v1.2.3-rc.08 "$new_main_sha"
git -C "$work" -c tag.gpgSign=false tag -a -m other mobile-v1.2.4-rc.99 "$new_main_sha"
git -C "$work" push -q origin \
  refs/tags/mobile-v1.2.3-rc.7 refs/tags/mobile-v1.2.3-rc.08 \
  refs/tags/mobile-v1.2.4-rc.99
run_release "$operator" candidate 1.2.3
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v1.2.3-rc.8^{commit}')" == \
   "$new_main_sha" ]]

# Publication pushes exactly one new ref, the annotated candidate tag.
refs_before="$(git --git-dir="$remote" for-each-ref --format='%(refname) %(objectname)')"
run_release "$operator" candidate 9.9.1 > "$tmp/one-ref-output"
grep -Fq 'Published mobile-v9.9.1-rc.1' "$tmp/one-ref-output"
refs_after="$(git --git-dir="$remote" for-each-ref --format='%(refname) %(objectname)')"
[[ "$(diff <(printf '%s\n' "$refs_before") <(printf '%s\n' "$refs_after") | grep -c '^[<>]')" == 1 ]]
[[ "$(git --git-dir="$remote" cat-file -t refs/tags/mobile-v9.9.1-rc.1)" == tag ]]
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v9.9.1-rc.1^{commit}')" == \
   "$(git --git-dir="$remote" rev-parse refs/heads/main)" ]]
if grep -Eq 'buzz-release-bot|workflow' "$tmp/one-ref-output"; then
  fail "publication output still refers to the App or a workflow"
fi

assert_no_candidate_tag() {
  local version="$1"
  if git --git-dir="$remote" show-ref --verify --quiet "refs/tags/mobile-v$version-rc.1"; then
    fail "mobile-v$version-rc.1 exists on the remote"
  fi
  [[ -z "$(git -C "$operator" tag -l "mobile-v$version-rc.*")" ]] || \
    fail "a local mobile-v$version-rc tag was left behind"
}

# A tag ruleset the operator cannot bypass fails before any tag is created.
# An absent bypass value is not accepted as success either.
if GH_CURRENT_USER_CAN_BYPASS=never run_release "$operator" candidate 9.9.7 > "$tmp/no-bypass-output" 2>&1; then
  fail "candidate succeeded without bypass of the mobile tag ruleset"
fi
grep -Fq 'mobile tag ruleset 1001' "$tmp/no-bypass-output"
assert_no_candidate_tag 9.9.7
if GH_CURRENT_USER_CAN_BYPASS='' run_release "$operator" candidate 9.9.7 >/dev/null 2>&1; then
  fail "candidate succeeded with an empty bypass report"
fi
assert_no_candidate_tag 9.9.7

# Any other ruleset drift also fails before a tag is created.
if GH_TAG_RULESET_STATE=disabled run_release "$operator" candidate 9.9.7 >/dev/null 2>&1; then
  fail "candidate succeeded with an inactive mobile tag ruleset"
fi
if GH_TAG_RULESET_TARGET=branch run_release "$operator" candidate 9.9.7 >/dev/null 2>&1; then
  fail "candidate succeeded with a mobile ruleset that targets branches"
fi
if GH_TAG_RULE_TYPES=creation,deletion run_release "$operator" candidate 9.9.7 >/dev/null 2>&1; then
  fail "candidate succeeded with weakened mobile tag rules"
fi
if GH_TAG_INCLUDES='refs/tags/desktop-v*' run_release "$operator" candidate 9.9.7 >/dev/null 2>&1; then
  fail "candidate succeeded with a ruleset that does not cover mobile tags"
fi
if GH_TAG_EXCLUDES='refs/tags/mobile-v9*' run_release "$operator" candidate 9.9.7 >/dev/null 2>&1; then
  fail "candidate succeeded with a ruleset that excludes mobile tags"
fi
assert_no_candidate_tag 9.9.7
# The sole tag bypass actor must be the authenticated fork operator.
if GH_TAG_ACTOR_ID=88 run_release "$operator" candidate 9.9.7 >/dev/null 2>&1; then
  fail "candidate accepted another tag policy actor"
fi
assert_no_candidate_tag 9.9.7

# The narrower refs/tags/mobile-v* include also covers candidate tags.
GH_TAG_INCLUDES='refs/tags/mobile-v*' run_release "$operator" candidate 9.9.8 >/dev/null
[[ "$(git --git-dir="$remote" cat-file -t refs/tags/mobile-v9.9.8-rc.1)" == tag ]]

# Missing/invalid policy, actor, source checks and producer evidence must stop
# the actual script before it creates a tag locally or in the bare remote.
for entry in \
  'BUZZ_MOBILE_RELEASE_TAG_RULESET_ID=' \
  'BUZZ_MOBILE_RELEASE_TAG_RULESET_ID=not-a-number' \
  'BUZZ_MOBILE_RELEASE_BRANCH_RULESET_ID=' \
  'GH_ACTOR=another-operator' \
  'GH_BRANCH_POLICY={}' \
  'GH_CHECK_MISSING=1' \
  'GH_CHECK_CONCLUSION=failure' \
  'GH_CHECK_CONCLUSION=skipped' \
  'GH_CHECK_STATUS=in_progress' \
  'GH_CHECK_APP=999' \
  'GH_DCO_CHECK_APP=3999' \
  'GH_DCO_CHECK_MISSING=1' \
  'GH_DCO_NEWER_FAILURE=1' \
  'GH_POLICY_DCO_APP=3002' \
  'GH_POLICY_DCO_ABSENT=1' \
  'GH_POLICY_DCO_APP=null' \
  'GH_POLICY_DCO_APP=0' \
  'GH_POLICY_DCO_APP=-1' \
  'GH_POLICY_DCO_APP=1.5' \
  'GH_POLICY_DCO_APP="3001"' \
  'GH_POLICY_DCO_APP=""' \
  'GH_POLICY_DCO_APP=true' \
  'GH_POLICY_MOBILE_APP=3001' \
  'GH_EXTRA_REQUIRED_CONTEXT=1' \
  'GH_CHECK_SHA=0000000000000000000000000000000000000000' \
  'GH_RUN_SHA=0000000000000000000000000000000000000000' \
  'GH_RUN_REPO=another/fork' \
  'GH_RUN_PATH=.github/workflows/untrusted.yml' \
  'GH_CHECK_NEWER_FAILURE=1'; do
  if (export "$entry"; run_release "$operator" candidate 9.9.7 > "$tmp/refusal-output" 2>&1); then
    fail "candidate accepted unqualified policy/source: $entry"
  fi
  assert_no_candidate_tag 9.9.7
  printf 'refused before tag: %s\n' "$entry"
done

# Matching a non-GHA policy/check pair cannot authorize an allowlisted CI gate.
if GH_POLICY_MOBILE_APP=3001 GH_CHECK_APP=3001 run_release "$operator" candidate 9.9.7 > "$tmp/non-gha" 2>&1; then
  fail "Mobile context accepted a non-GitHub-Actions producer"
fi
assert_no_candidate_tag 9.9.7

# The policy, rather than an inherited upstream App ID, selects DCO authority.
# Policy/check values are independently supplied; mismatches above must fail.
GH_POLICY_DCO_APP=3002 GH_DCO_CHECK_APP=3002 run_release "$operator" candidate 6.6.1 > "$tmp/alternate-dco" 2>&1
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v6.6.1-rc.1^{commit}')" == "$new_main_sha" ]]
printf 'qualified configured alternate DCO producer: 3002\n'

# A rejected push (the operator is not in the bypass team) fails, explains why,
# and removes the local tag so a retry is clean.
# A machine-wide core.hooksPath would shadow the bare remote's own hooks.
git --git-dir="$remote" config core.hooksPath "$remote/hooks"
cat > "$remote/hooks/pre-receive" <<'HOOK'
#!/usr/bin/env bash
echo "remote: rejected by repository rule (test)" >&2
exit 1
HOOK
chmod +x "$remote/hooks/pre-receive"
if run_release "$operator" candidate 9.9.6 > "$tmp/push-rejected-output" 2>&1; then
  fail "candidate succeeded despite a rejected tag push"
fi
grep -Fq 'mobile tag ruleset 1001' "$tmp/push-rejected-output"
assert_no_candidate_tag 9.9.6
rm "$remote/hooks/pre-receive"
# The retry after the rejection is clean and gets the same candidate number.
run_release "$operator" candidate 9.9.6 >/dev/null
[[ "$(git --git-dir="$remote" cat-file -t refs/tags/mobile-v9.9.6-rc.1)" == tag ]]

# Explicit historical source remains pinned even when the source branch advances.
if GH_ONLY_QUALIFIED_SHA="$new_main_sha" run_release "$operator" candidate 7.7.3 --commit "$remote_main_sha" > "$tmp/historical-checks" 2>&1; then
  fail "historical source borrowed newer tip qualification"
fi
assert_no_candidate_tag 7.7.3
grep -Fq "trusted required check did not succeed for $remote_main_sha" "$tmp/historical-checks"
printf 'refused before tag: historical source cannot borrow newer tip checks\n'
run_release "$operator" candidate 7.7.0 --commit "$remote_main_sha"
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v7.7.0-rc.1^{commit}')" == "$remote_main_sha" ]]
run_release "$operator" candidate 1.2.5 --commit "$remote_main_sha" --source-branch mobile-v1.2
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v1.2.5-rc.1^{commit}')" == "$remote_main_sha" ]]
tags_before="$(git --git-dir="$remote" for-each-ref refs/tags)"
if run_release "$operator" candidate 7.7.1 --commit "$stable_sha" >/dev/null 2>&1; then
  fail "accepted a commit outside main history"
fi
if run_release "$operator" candidate 1.2.6 --source-branch mobile-v1.2 --commit "$new_main_sha" >/dev/null 2>&1; then
  fail "accepted a main-only commit outside stable history"
fi
for invalid_sha in main HEAD deadbeef 9999999999999999999999999999999999999999 ''; do
  if run_release "$operator" candidate 7.7.1 --commit "$invalid_sha" >/dev/null 2>&1; then
    fail "accepted invalid or absent commit '$invalid_sha'"
  fi
done
if GH_COMPARE_FAIL=1 run_release "$operator" candidate 7.7.1 --commit "$remote_main_sha" >/dev/null 2>&1; then
  fail "accepted unavailable ancestry evidence"
fi
if run_release "$operator" candidate 7.7.1 --commit "$remote_main_sha" --commit "$new_main_sha" >/dev/null 2>&1; then
  fail "accepted duplicate commit options"
fi
[[ "$(git --git-dir="$remote" for-each-ref refs/tags)" == "$tags_before" ]]
[[ -z "$(git -C "$operator" tag -l 'mobile-v7.7.1-rc.*' 'mobile-v1.2.6-rc.*')" ]]

# Publishing through a fork is rejected and unsupported gh versions fail before
# publishing any candidate tag.
fork_operator="$tmp/fork-operator"
git clone -q "$remote" "$fork_operator"
git -C "$fork_operator" config user.name test
git -C "$fork_operator" config user.email test@example.com
if (cd "$fork_operator" && "$script" candidate 2.0.0 >/dev/null 2>&1); then
  fail "noncanonical origin was accepted"
fi
# A canonical fetch URL with a fork push destination (pushurl or
# pushInsteadOf) is rejected before any tag is created locally or anywhere.
fork_remote="$tmp/fork.git"
git clone -q --bare "$remote" "$fork_remote"
fork_tags_before="$(git --git-dir="$fork_remote" for-each-ref refs/tags)"
git -C "$operator" config remote.origin.pushurl "file://$fork_remote"
if run_release "$operator" candidate 2.0.1 > "$tmp/pushurl-output" 2>&1; then
  fail "candidate succeeded with origin pushing to a fork"
fi
git -C "$operator" config --unset remote.origin.pushurl
grep -Fq 'origin pushes somewhere other than it fetches from' "$tmp/pushurl-output"
# A second pushurl after a canonical first one would also receive the tag.
git -C "$operator" config --add remote.origin.pushurl "$canonical_origin"
git -C "$operator" config --add remote.origin.pushurl "file://$fork_remote"
if run_release "$operator" candidate 2.0.1 > "$tmp/pushurl-output" 2>&1; then
  fail "candidate succeeded with an extra push URL pointing at a fork"
fi
git -C "$operator" config --unset-all remote.origin.pushurl
grep -Fq 'origin pushes somewhere other than it fetches from' "$tmp/pushurl-output"
git -C "$operator" config "url.file://$fork_remote.pushInsteadOf" "$canonical_origin"
if run_release "$operator" candidate 2.0.1 >/dev/null 2>&1; then
  fail "candidate succeeded with pushInsteadOf pointing at a fork"
fi
git -C "$operator" config --unset "url.file://$fork_remote.pushInsteadOf"
assert_no_candidate_tag 2.0.1
[[ "$(git --git-dir="$fork_remote" for-each-ref refs/tags)" == "$fork_tags_before" ]]
# An insteadOf rewrite sends both fetch and push to the fork. GitHub then has
# no tag, so publication fails loudly and keeps the local tag as evidence.
git -C "$operator" config "url.file://$fork_remote.insteadOf" "$canonical_origin"
if (cd "$operator" && "$script" candidate 2.0.1 > "$tmp/insteadof-output" 2>&1); then
  fail "candidate reported success after publishing into a fork"
fi
git -C "$operator" config --unset "url.file://$fork_remote.insteadOf"
grep -Fq 'origin must point to authorized reynholm/buzz' "$tmp/insteadof-output"
assert_no_candidate_tag 2.0.1
# GitHub being unavailable after a successful push also stops without deleting
# the local tag, which may already be published.
if GH_TAG_REF_FAIL=1 run_release "$operator" candidate 2.0.2 > "$tmp/api-down-output" 2>&1; then
  fail "candidate reported success without GitHub confirmation"
fi
grep -Fq 'could not confirm mobile-v2.0.2-rc.1 on authorized reynholm/buzz' "$tmp/api-down-output"
[[ "$(git -C "$operator" cat-file -t refs/tags/mobile-v2.0.2-rc.1)" == tag ]] || \
  fail "the local tag was deleted while GitHub was unavailable"
[[ "$(git --git-dir="$remote" cat-file -t refs/tags/mobile-v2.0.2-rc.1)" == tag ]]
# Rerunning after that numbers from the remote tags: rc.1 stays as published
# and the rerun publishes rc.2.
rc1_kept_oid="$(git --git-dir="$remote" rev-parse refs/tags/mobile-v2.0.2-rc.1)"
run_release "$operator" candidate 2.0.2 >/dev/null
[[ "$(git --git-dir="$remote" rev-parse refs/tags/mobile-v2.0.2-rc.1)" == "$rc1_kept_oid" ]]
[[ "$(git --git-dir="$remote" cat-file -t refs/tags/mobile-v2.0.2-rc.2)" == tag ]]

# push.followTags must not publish other local annotated tags: a stray local
# mobile tag on an ancestor stays unpublished.
git -C "$operator" fetch -q origin main
git -C "$operator" -c tag.gpgSign=false tag -a -m stray mobile-v9.9.9-rc.99 "$remote_main_sha"
git -C "$operator" config push.followTags true
refs_before="$(git --git-dir="$remote" for-each-ref --format='%(refname)')"
run_release "$operator" candidate 2.0.3 >/dev/null
git -C "$operator" config --unset push.followTags
git -C "$operator" tag -d mobile-v9.9.9-rc.99 >/dev/null
[[ "$(diff <(printf '%s\n' "$refs_before") \
  <(git --git-dir="$remote" for-each-ref --format='%(refname)') | grep -c '^[<>]')" == 1 ]] || \
  fail "candidate publication pushed more than one ref"
if git --git-dir="$remote" show-ref --verify --quiet refs/tags/mobile-v9.9.9-rc.99; then
  fail "push.followTags published a stray local mobile tag"
fi

# A push that lands but reports failure keeps the local tag and goes on to the
# canonical GitHub check, which confirms the candidate.
GIT_PUSH_LANDS_BUT_FAILS=1 run_release "$operator" candidate 2.0.4 > "$tmp/landed-output" 2>&1
grep -Fq 'reported failure, but origin has mobile-v2.0.4-rc.1' "$tmp/landed-output"
grep -Fq 'Published mobile-v2.0.4-rc.1' "$tmp/landed-output"
[[ "$(git -C "$operator" cat-file -t refs/tags/mobile-v2.0.4-rc.1)" == tag ]] || \
  fail "the local tag was deleted although the candidate landed"
# The same ambiguous push into a fork (origin rewritten by insteadOf) must not
# be declared usable: GitHub has no such tag.
git -C "$operator" config "url.file://$fork_remote.insteadOf" "$canonical_origin"
if (cd "$operator" && GIT_PUSH_LANDS_BUT_FAILS=1 "$script" candidate 2.0.5 > "$tmp/landed-fork-output" 2>&1); then
  fail "an ambiguous push into a fork was reported as published"
fi
git -C "$operator" config --unset "url.file://$fork_remote.insteadOf"
grep -Fq 'origin must point to authorized reynholm/buzz' "$tmp/landed-fork-output"
assert_no_candidate_tag 2.0.5

# A shallow operator clone can still pin a verified ancestor outside its
# local history.
shallow_operator="$tmp/shallow-operator"
git -c "url.file://$remote.insteadOf=$canonical_origin" \
  -c protocol.file.allow=always clone -q --depth 1 "$canonical_origin" "$shallow_operator"
git -C "$shallow_operator" config tag.gpgSign false
git -C "$shallow_operator" config user.name test
git -C "$shallow_operator" config user.email test@example.com
if git -C "$shallow_operator" cat-file -e "$remote_main_sha^{commit}" 2>/dev/null; then
  fail "shallow-clone fixture already contains the historical commit"
fi
run_release "$shallow_operator" candidate 7.7.2 --commit "$remote_main_sha" >/dev/null
[[ "$(git --git-dir="$remote" rev-parse 'refs/tags/mobile-v7.7.2-rc.1^{commit}')" == "$remote_main_sha" ]]

tags_before="$(git --git-dir="$remote" for-each-ref refs/tags)"
if GH_VERSION=2.86.0 run_release "$operator" candidate 2.0.0 >/dev/null 2>&1; then
  fail "gh older than 2.87.0 was accepted"
fi
if GH_VERSION=2.9.0 run_release "$operator" candidate 2.0.0 >/dev/null 2>&1; then
  fail "numeric gh version comparison accepted 2.9.0 as at least 2.87.0"
fi
[[ "$(git --git-dir="$remote" for-each-ref refs/tags)" == "$tags_before" ]]

# Dirty trees and invalid marketing versions fail before publication.
echo dirty > "$operator/untracked"
if run_release "$operator" candidate 2.0.0 >/dev/null 2>&1; then
  fail "dirty operator tree was accepted"
fi
rm "$operator/untracked"
if run_release "$operator" candidate 1.2 >/dev/null 2>&1; then
  fail "invalid marketing version was accepted"
fi
if run_release "$operator" candidate 01.2.3 >/dev/null 2>&1; then
  fail "marketing version with a leading zero was accepted"
fi

# Mobile no longer has release branches, finalization, a stable alias, or a
# GitHub Release call, and no GitHub App or workflow publishes tags: the
# operator pushes the tag directly.
if run_release "$operator" start 2.0.0 >/dev/null 2>&1; then
  fail "removed start command was accepted"
fi
if run_release "$operator" finalize 1.2.3-rc.2 >/dev/null 2>&1; then
  fail "removed finalize command was accepted"
fi
if git --git-dir="$remote" for-each-ref --format='%(refname)' refs/heads/mobile-release/ | grep -q .; then
  fail "mobile release branch was created"
fi
if git --git-dir="$remote" show-ref --verify --quiet refs/tags/mobile-v1.2.3; then
  fail "stable mobile tag alias was created"
fi
assert_no_removed_mobile_release_behavior \
  "$repo_root/scripts/mobile-release.sh"
for removed in .github/workflows/mobile-release-candidate.yml \
    scripts/publish-mobile-release-candidate.sh \
    scripts/test-mobile-release-candidate-publisher.sh; do
  [[ ! -e "$repo_root/$removed" ]] || fail "$removed must stay removed"
done
if grep -Eq 'buzz-release-bot|gh[[:space:]]+workflow' "$repo_root/scripts/mobile-release.sh"; then
  fail "mobile-release.sh must not use the release App or dispatch a workflow"
fi
# Prove the negative contract itself is discriminating rather than merely
# accepting a missing or broken search tool as "not found."
printf '%s\n' 'gh release create forbidden' > "$tmp/forbidden-mobile-release-behavior"
if (assert_no_removed_mobile_release_behavior "$tmp/forbidden-mobile-release-behavior") \
    >/dev/null 2>&1; then
  fail "removed-behavior assertion did not reject a forbidden GitHub Release call"
fi
grep -Fq 'version: 0.0.0+1' "$repo_root/mobile/pubspec.yaml"
if grep -qE 'release-mobile|bump-mobile-version|get-current-mobile-version' "$repo_root/Justfile"; then
  fail "metadata-only mobile release recipe remains in Justfile"
fi

echo "mobile release contract passed"
