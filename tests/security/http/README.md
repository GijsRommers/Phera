# Security tests for `phera serve`

Defensive security tests for the built-in HTTP server: what a hostile client
can reach between the socket and a running PHP script. They are plain Python
3 (standard library, raw sockets) plus one shell runner; they only ever talk
to `127.0.0.1` on a port you choose, and only to a server the runner started
itself.

## Layout

| File | What it is |
| --- | --- |
| `run_all.sh` | Builds a fixture site in a temp dir, starts `phera serve --listen 127.0.0.1:18400 --workers 1`, runs everything, stops the server. |
| `repro_f1_source_disclosure.py` | F1: `/secret.PHP` (case-variant extension) is served as source instead of executed. |
| `repro_f2_content_length_framing.py` | F2: script-set `Content-Length` is sent verbatim while the full body follows on a keep-alive connection (response splitting). |
| `repro_f3_no_memory_limit.py` | F3: one request can grow server memory without bound (no memory_limit / max_execution_time). |
| `repro_f4_no_body_timeout.py` | F4: a trickled request body is held open forever (no body/idle timeout). |
| `repro_f5_buffered_response_pinning.py` | F5: clients that stop reading pin whole buffered responses in memory. |
| `repro_f6_post_on_get.py` | F6: `$_POST` is filled for GET/PUT form bodies (PHP only does POST). |
| `repro_f7_error_path_leak.py` | F7: error bodies leak absolute filesystem paths. |
| `repro_f8_lowercase_post_method.py` | F8 (round 2): `$_POST` filled for case-variant method tokens (`post`), which PHP rejects (methods are case-sensitive). |
| `repro_f9_duplicate_content_type.py` | F9 (round 2): duplicate `Content-Type` headers make `$_POST` and `$_SERVER['CONTENT_TYPE']` disagree. |
| `attack_paths.py` | Area 1 battery: traversal encodings, hidden files, symlinks, source disclosure shapes. |
| `attack_parsing.py` | Area 2 battery: CL/TE conflicts, chunked malformations, control bytes, pipelining. |
| `attack_headers.py` | Area 3 battery: CRLF/NUL/control-byte payloads through `header()`. |
| `attack_superglobals.py` | Area 5 battery: nesting/param caps, duplicate keys, `$_SERVER` spoofing. |
| `attack_isolation.py` | Area 6: per-request state on a single worker, across fatal errors, exit, disconnects. |
| `attack_resources.py` | Area 4 battery (bounded): slow headers, huge declared bodies, idle keep-alives, 500 connections. |
| `fuzz.py` | Area 8: mutation fuzzer over recorded valid requests; run for hours at low priority. (Round 2 fixed its `alive()` check, which passed an int to `status_of` and always aborted at the first checkpoint.) |
| `lib.py` | Shared raw-socket helpers. |

## Running

```sh
cargo build --release -p phera-cli          # from the repository root
tests/security/http/run_all.sh              # uses target/release/phera, port 18400
```

Each reproducer exits 1 when the vulnerability is present and prints the
evidence (exact status, headers, body). A single one can be run against any
local server that serves the same fixture shape:

```sh
phera serve --listen 127.0.0.1:18400 --workers 1 /tmp/my-site
python3 tests/security/http/repro_f1_source_disclosure.py 18400
PHERA_PID=$(pgrep -f "phera serve --listen 127.0.0.1:18400") \
  python3 tests/security/http/repro_f3_no_memory_limit.py 18400
```

`PHERA_PID` (the server's pid) is needed by the two reproducers that sample
resident memory. `repro_f4` waits 35 s by design (it must outlast the 30 s
header timeout); pass a shorter time as the second argument to make it
quicker.

The long-running fuzzer is meant to run for hours, nicely:

```sh
phera serve --listen 127.0.0.1:18400 --workers 1 /tmp/my-site &
nice -n 15 python3 tests/security/http/fuzz.py 18400 7200 42
```

Keep floods under 60 s and under 2000 open connections, and run the server
with `--workers 1` or `--workers 2` when testing locally on a shared machine.

## Scope and limits

These tests cover the HTTP server and the request/response boundary only
(path routing, static files, parsing, superglobals, response functions,
per-request isolation, resource limits, robustness). Hostile PHP source given
to the engine is a separate task's scope. Everything here is non-destructive
and loopback-only.
