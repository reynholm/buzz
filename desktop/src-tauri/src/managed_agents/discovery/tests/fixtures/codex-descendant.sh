#!/bin/sh
/bin/sleep 60 &
printf '%s\n' "$!" > "${0%/*}/descendant.pid"
printf '@agentclientprotocol/codex-acp 1.1.2\n'
exit 0
