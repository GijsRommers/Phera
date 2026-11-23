#!/usr/bin/env python3
"""F5: responses are fully buffered in memory before sending, and a client
that stops reading pins the whole buffer with no write timeout.

N clients each request a large static file and read only the first KB; the
server has already read the whole file into memory (static files are read
with no size cap) and holds it until the socket write completes -- which
never happens while the client does not read. 5 x 50 MB = ~250 MB pinned by
five small requests.

Expect (fixed): bounded buffering / write timeout / streaming, so a
non-reading client cannot pin the whole response.

Usage: repro_f5_buffered_response_pinning.py [port]  (needs PHERA_PID and a
big.bin in the served site, as run_all.sh creates.)
"""
import os
import sys
import time

from lib import connect, get, port_arg, report, server_rss_kb, status_of

port = port_arg(sys.argv)
pid = os.environ.get("PHERA_PID")
socks = []
for _ in range(5):
    s = connect(port, timeout=30)
    s.sendall(b"GET /big.bin HTTP/1.1\r\nHost: x\r\n\r\n")
    s.settimeout(2)
    try:
        s.recv(1024)  # read a little, then stop reading
    except socket.timeout:
        pass
    socks.append(s)
peak = 0
t0 = time.time()
while time.time() - t0 < 8:
    rss = server_rss_kb(pid)
    if rss:
        peak = max(peak, rss)
    time.sleep(0.5)
control = status_of(get(port, "/hello.php"))[0]
for s in socks:
    try:
        s.close()
    except OSError:
        pass
vulnerable = peak > 150 * 1024 and control == 200
sys.exit(
    report(
        "5 clients read 1KB of a 50MB response",
        vulnerable,
        f"server peak RSS {peak/1024:.0f} MB while held, control request {control}",
    )
)
