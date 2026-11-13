#!/usr/bin/env bash

require_mobile_release_source() {
  local version="$1" branch="$2"
  [[ "$branch" == main || "$branch" == "mobile-v${version%.*}" ]] || {
    echo "Source branch must be main or mobile-v${version%.*} for version $version" >&2
    return 1
  }
}

# Check every source branch, including main. No release actor can bypass its
# source review/check policy. GitHub Actions producers are pinned below; the
# authenticated fork policy explicitly chooses the DCO producer. Its authority
# must be established by the operator when configuring protection.
require_mobile_release_branch_ruleset() {
  local branch="$1" policy
  policy="$(mobile_release_api "repos/$MOBILE_RELEASE_REPOSITORY/rulesets/$MOBILE_RELEASE_BRANCH_RULESET_ID")" || {
    fail_release_ruleset "could not verify configured mobile branch protection"; return 1
  }
  MOBILE_RELEASE_REQUIRED_CHECKS="$(jq -ce --arg branch "$branch" '
    {"Desktop E2E Integration":15368,"Desktop":15368,"Rust Lint":15368,
     "Security":15368,"Unit Tests":15368,"Windows Rust (x86_64-pc-windows-msvc)":15368,
     "Mobile":15368,"Web":15368,"Backend Integration (relay e2e)":15368,
     "Desktop E2E Relay":15368,"Relay E2E":15368,"Desktop Build (macOS)":15368,
     "PostgreSQL Tests":15368} as $producers |
    select(.target == "branch" and .enforcement == "active" and .bypass_actors == []
      and .conditions.ref_name.exclude == []
      and (.conditions.ref_name.include | index("refs/heads/main") != null)
      and (.conditions.ref_name.include | index("refs/heads/" + $branch) != null
        or ($branch != "main" and index("refs/heads/mobile-v*") != null))
      and any(.rules[]; .type == "deletion")
      and any(.rules[]; .type == "non_fast_forward")
      and any(.rules[]; .type == "pull_request"
        and .parameters.required_approving_review_count >= 1
        and .parameters.dismiss_stale_reviews_on_push == true
        and .parameters.require_last_push_approval == true
        and .parameters.required_review_thread_resolution == true)) |
    [.rules[] | select(.type == "required_status_checks"
      and .parameters.strict_required_status_checks_policy == true)
      | .parameters.required_status_checks[]] |
    select(length > 0
      and any(.[]; .context == "Mobile") and any(.[]; .context == "DCO Check")
      and all(.[]; if .context == "DCO Check" then
        (.integration_id | if type == "number" then . > 0 and floor == . else false end)
      else
        .integration_id == $producers[.context] and .integration_id != null
      end))
  ' <<<"$policy")" || {
    fail_release_ruleset "configured mobile branch protection must enforce review and trusted required checks without bypass, including main"
    return 1
  }
}

require_mobile_release_commit_ancestor() {
  local target_sha="$1" source_sha="$2" merge_base
  merge_base="$(mobile_release_api "repos/$MOBILE_RELEASE_REPOSITORY/compare/$target_sha...$source_sha" --jq .merge_base_commit.sha)" || {
    echo "Could not verify candidate source ancestry" >&2; return 1
  }
  [[ "$merge_base" == "$target_sha" ]] || {
    echo "Candidate commit $target_sha is not in selected source history at $source_sha" >&2
    return 1
  }
}

# Qualification is for the selected immutable commit, even if the checked-out
# HEAD or remote branch tip differs. A copied check name, stale successful
# attempt, skipped job or another workflow cannot qualify it.
require_mobile_release_qualification() {
  local sha="$1" checks workflow workflow_id context producer check details run_id run suite
  checks="$(mobile_release_api --paginate --slurp \
    "repos/$MOBILE_RELEASE_REPOSITORY/commits/$sha/check-runs?filter=latest&per_page=100")" || {
    fail_release_ruleset "could not read exact-source mobile qualification"; return 1
  }
  workflow="$(mobile_release_api "repos/$MOBILE_RELEASE_REPOSITORY/actions/workflows/ci.yml")" || return 1
  workflow_id="$(jq -er 'select(.path == ".github/workflows/ci.yml") | .id | select(type == "number" and . > 0)' <<<"$workflow")" || return 1
  while IFS=$'\t' read -r context producer; do
    check="$(jq -ce --arg name "$context" --arg sha "$sha" --argjson producer "$producer" '
      [.[].check_runs[] | select(.name == $name and .app.id == $producer)] |
      sort_by(.id) | last |
      select(.head_sha == $sha and .status == "completed" and .conclusion == "success")
    ' <<<"$checks")" || {
      fail_release_ruleset "trusted required check did not succeed for $sha: $context"; return 1
    }
    if [[ "$producer" == 15368 ]]; then
      details="$(jq -er .details_url <<<"$check")" || return 1
      [[ "$details" =~ ^https://github\.com/reynholm/buzz/actions/runs/([0-9]+)/job/[0-9]+$ ]] || {
        fail_release_ruleset "untrusted required check workflow: $context"; return 1
      }
      run_id="${BASH_REMATCH[1]}"
      suite="$(jq -er .check_suite.id <<<"$check")" || return 1
      run="$(mobile_release_api "repos/$MOBILE_RELEASE_REPOSITORY/actions/runs/$run_id")" || return 1
      jq -e --arg sha "$sha" --arg repo "$MOBILE_RELEASE_REPOSITORY" \
        --argjson workflow "$workflow_id" --argjson suite "$suite" '
        .head_sha == $sha and .head_repository.full_name == $repo
        and .workflow_id == $workflow and .path == ".github/workflows/ci.yml"
        and .check_suite_id == $suite and (.event == "push" or .event == "pull_request")
        and .status == "completed" and .conclusion == "success"
      ' <<<"$run" >/dev/null || {
        fail_release_ruleset "untrusted required check workflow/source: $context"; return 1
      }
    fi
  done < <(jq -r '.[] | [.context,.integration_id] | @tsv' <<<"$MOBILE_RELEASE_REQUIRED_CHECKS")
}
