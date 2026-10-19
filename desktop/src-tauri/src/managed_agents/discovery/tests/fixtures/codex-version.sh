#!/bin/sh
IFS= read -r version < "${0%/*}/version" || exit 1
printf '@agentclientprotocol/codex-acp %s\n' "$version"
