# Kimi ACP fixtures

`permission_request.json` preserves an actual successful black-box permission
exchange from Kimi Code CLI 2.3.1. Session and tool-call IDs are normalized. The
request's numeric JSON-RPC ID, option IDs/kinds and absence of rawInput are
retained. This captures neither an incident nor a provider failure.

Regression adapters mutate this successful frame to exercise stale sessions,
duplicates, broken writes, silence and process exit through production pipes.
Those faults are simulations. OAuth failure payloads in the tests are explicitly
synthetic because the historical failure trace is unavailable.
