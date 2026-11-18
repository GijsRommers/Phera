#!/usr/bin/env python3
"""F2: a script-set `Content-Length` response header is sent verbatim while
the whole body is still written, and the keep-alive connection stays open.

`header('Content-Length: 5')` from PHP reaches the wire unchanged; the server
then writes all 101 body bytes. Any framing-respecting reader (proxy, cache,
keep-alive client) treats the first 5 bytes as the whole body and parses the
remaining 96 bytes as the next response on the same connection: response
splitting / desync. PHP's own dev server closes the connection in this
situation; a correct server must control the framing headers itself.

Expect (fixed): the server ignores or corrects a script-set Content-Length,
or closes the connection; never `content-length: 5` + 101 bytes on a
keep-alive connection.
"""
import socket
import sys

from lib import HOST, connect, port_arg, report

port = port_arg(sys.argv)
s = connect(port, timeout=8)
s.sendall(b"GET /cl.php HTTP/1.1\r\nHost: x\r\n\r\n")  # keep-alive, no close
buf = b""
while b"\r\n\r\n" not in buf:
    buf += s.recv(4096)
head, rest = buf.split(b"\r\n\r\n", 1)
cl = None
for line in head.split(b"\r\n")[1:]:
    if line.lower().startswith(b"content-length:"):
        cl = int(line.split(b":")[1])
while len(rest) < 4096:
    try:
        c = s.recv(4096)
    except socket.timeout:
        break
    if not c:
        break
    rest += c
still_open = b"connection: close" not in head.lower()
vulnerable = cl is not None and len(rest) > cl and still_open
s.close()
sys.exit(
    report(
        "GET /cl.php (keep-alive)",
        vulnerable,
        f"declared content-length={cl}, body bytes on wire={len(rest)}, "
        f"connection still open={still_open}; head={head[:100]!r}",
    )
)
