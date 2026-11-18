#!/usr/bin/env python3
"""F3: no per-request memory limit and no execution timeout.

One request to a script that allocates in a loop grows the server's resident
memory without bound (PHP's defaults memory_limit=128M and
max_execution_time=30s prevent exactly this). Here grow_bounded.php stops
itself after ~400 MB so the reproducer is safe to run anywhere; the server
has no cap, so an unbounded loop reaches the kernel OOM killer and takes the
whole process (every worker) down.

Expect (fixed): the request is stopped by a memory/execution limit well
below what one request can allocate.

Usage: repro_f3_no_memory_limit.py [port]   (PHERA_PID env gives the server
pid for the RSS check; without it the reproducer only reports timing.)
"""
import os
import socket
import sys
import time

from lib import connect, port_arg, report, server_rss_kb, build_request

port = port_arg(sys.argv)
pid = os.environ.get("PHERA_PID")
s = connect(port, timeout=30)
s.sendall(build_request("GET", "/grow_bounded.php", [("Host", "x")]))
t0 = time.time()
peak = 0
while time.time() - t0 < 25:
    rss = server_rss_kb(pid)
    if rss:
        peak = max(peak, rss)
    try:
        s.settimeout(1)
        if s.recv(65536) == b"":
            break
    except socket.timeout:
        pass
    except OSError:
        break
    time.sleep(0.25)
s.close()
evidence = f"one request ran {time.time()-t0:.1f}s, server peak RSS {peak/1024:.0f} MB (PHP default memory_limit would cap a request at 128 MB)"
vulnerable = peak > 300 * 1024  # >300 MB from a single request
sys.exit(report("GET /grow_bounded.php (allocates ~400 MB)", vulnerable, evidence))
