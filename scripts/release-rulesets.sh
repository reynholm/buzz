#!/usr/bin/env bash

# This checkout's release authority. Credentials and Git attribution remain
# separate: the authenticated GitHub operator never supplies a Git author.
readonly MOBILE_RELEASE_REPOSITORY=reynholm/buzz
readonly MOBILE_RELEASE_OPERATOR=reynholm
# The fork has no configured release policies yet. Missing IDs fail closed;
# upstream IDs must never silently authorize a fork publication.
readonly MOBILE_RELEASE_TAG_RULESET_ID="${BUZZ_MOBILE_RELEASE_TAG_RULESET_ID:-}"
readonly MOBILE_RELEASE_BRANCH_RULESET_ID="${BUZZ_MOBILE_RELEASE_BRANCH_RULESET_ID:-}"

fail_release_ruleset() {
  echo "Error: $*" >&2
  return 1
}

mobile_release_api() {
  GH_HOST=github.com gh api "$@"
}

require_canonical_repository() {
  local origin_url effective_url
  origin_url="$(git config --get remote.origin.url 2>/dev/null)" || \
    fail_release_ruleset "origin must point to authorized $MOBILE_RELEASE_REPOSITORY" || return 1
  effective_url="$(git remote get-url origin)" || return 1
  # Check both configured and rewritten fetch destinations before any tag is
  # created. API readback alone is too late to prevent a push into another repo.
  for url in "$origin_url" "$effective_url"; do
    case "$url" in
      git@github.com:reynholm/buzz.git|ssh://git@github.com/reynholm/buzz.git|https://github.com/reynholm/buzz.git|https://github.com/reynholm/buzz) ;;
      *) fail_release_ruleset "origin must point to authorized $MOBILE_RELEASE_REPOSITORY, not '$url'"; return 1 ;;
    esac
  done
  [[ "$(git remote get-url --push --all origin)" == "$effective_url" ]] || \
    fail_release_ruleset "origin pushes somewhere other than it fetches from" || return 1
}

require_mobile_release_operator() {
  local actor
  [[ "$MOBILE_RELEASE_TAG_RULESET_ID" =~ ^[1-9][0-9]*$ &&
     "$MOBILE_RELEASE_BRANCH_RULESET_ID" =~ ^[1-9][0-9]*$ ]] || {
    fail_release_ruleset "configure numeric BUZZ_MOBILE_RELEASE_TAG_RULESET_ID and BUZZ_MOBILE_RELEASE_BRANCH_RULESET_ID for $MOBILE_RELEASE_REPOSITORY before publication"
    return 1
  }
  actor="$(mobile_release_api user)" || {
    fail_release_ruleset "could not verify the authenticated mobile release operator"; return 1
  }
  MOBILE_RELEASE_OPERATOR_ID="$(jq -er --arg login "$MOBILE_RELEASE_OPERATOR" \
    'select(.login == $login) | .id | select(type == "number" and . > 0)' <<<"$actor")" || {
    fail_release_ruleset "mobile publication requires authenticated operator $MOBILE_RELEASE_OPERATOR"; return 1
  }
}

require_mobile_release_tag_ruleset() {
  local policy
  policy="$(mobile_release_api "repos/$MOBILE_RELEASE_REPOSITORY/rulesets/$MOBILE_RELEASE_TAG_RULESET_ID")" || {
    fail_release_ruleset "could not verify configured mobile tag protection"; return 1
  }
  jq -e --argjson operator "$MOBILE_RELEASE_OPERATOR_ID" '
    .target == "tag" and .enforcement == "active" and .current_user_can_bypass == "always"
    and (.bypass_actors == [{actor_id:$operator,actor_type:"User",bypass_mode:"always"}])
    and ([.rules[].type] | sort == ["creation","deletion","non_fast_forward","update"])
    and (.conditions.ref_name.exclude == [])
    and (.conditions.ref_name.include | . == ["refs/tags/mobile-*"] or . == ["refs/tags/mobile-v*"])
  ' <<<"$policy" >/dev/null || {
    fail_release_ruleset "mobile tag ruleset $MOBILE_RELEASE_TAG_RULESET_ID must protect immutable mobile tags with only operator $MOBILE_RELEASE_OPERATOR as its bypass actor"
    return 1
  }
}
