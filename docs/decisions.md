# Decisions

This log records what Phera decided, why, and which alternatives it considered.
Newest entries appear first in each section.

## Server and requests

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | `phera serve` hides PHP error details in responses and logs them by default. `--dev` enables display. | Production logs retain diagnostics, including fatal traces. Local development can show the same errors in the response. | Displaying PHP errors in production by default. | Owner |
| 2026-11-19 | One deadline covers request headers and body. `--request-read-timeout` sets the initial budget, 60 seconds by default. Each 16 KiB received in the body adds a second. Expiry returns HTTP 408. | Credit for bytes received gives steady uploads time to finish. A trickle earns little credit, and the body-size limit bounds the extension. | A timeout that resets on each read, or a fixed deadline with no credit for upload progress. | Design |
| 2026-11-19 | Refuse multipart boundaries longer than 70 bytes and check for cancellation while parsing the body. | RFC 2046 limits the boundary to 70 bytes. Bounded scans let the parser stop when the request ends. | Accepting longer boundaries. | Owner |
