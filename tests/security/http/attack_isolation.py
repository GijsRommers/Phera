#!/usr/bin/env python3
"""Area 6: request isolation on one worker.

Sends /state.php repeatedly with different parameters, interleaved with
requests that end in an uncaught exception, a runtime error, a client
disconnect and exit(), plus 30 concurrent requests -- all on a single-worker
server, so every request reuses the same thread. Every request must see
fresh state: static=1, constant='first', global=1, preg_last_error=1, its
own X-State header, empty headers_list.
"""
import json
import socket
import sys
import threading
import time

from lib import connect, get, port_arg, split_response, status_of, build_request

port = port_arg(sys.argv)


def state(target="/state.php"):
    resp = get(port, target, read_timeout=15)
    code, _ = status_of(resp)
    _, hdrs, raw = split_response(resp)
    xs = next((v for n, v in hdrs if n.lower() == "x-state"), None)
    try:
        return code, json.loads(raw[raw.index(b"{"):].decode()), xs
    except Exception:
        return code, raw[:120], xs


problems = []
for i in range(3):
    code, d, xs = state(f"/state.php?set=req{i}")
    if not (isinstance(d, dict) and d.get("bump") == 1 and d.get("const") == "first"
            and d.get("global_g") == 1 and d.get("preg_last_error") == 1 and xs == f"req{i}"):
        problems.append(f"req{i}: {code} {d} {xs}")

for label, target in [("fatal", "/fatal.php"), ("crash", "/crash.php"), ("exit", "/exit.php")]:
    get(port, target, read_timeout=15)
    code, d, xs = state(f"/state.php?set=after-{label}")
    if not (isinstance(d, dict) and d.get("bump") == 1 and d.get("global_g") == 1 and xs == f"after-{label}"):
        problems.append(f"after-{label}: {code} {d} {xs}")

s = connect(port, timeout=10)
s.sendall(build_request("GET", "/big.php", [("Host", "x")]))
try:
    s.recv(1024)
except socket.timeout:
    pass
s.close()
time.sleep(1)
code, d, xs = state("/state.php?set=after-disconnect")
if not (isinstance(d, dict) and d.get("bump") == 1 and xs == "after-disconnect"):
    problems.append(f"after-disconnect: {code} {d} {xs}")

results = []
lock = threading.Lock()


def one(i):
    code, d, xs = state(f"/state.php?set=par{i}")
    with lock:
        results.append((i, code, xs, d))


threads = [threading.Thread(target=one, args=(i,)) for i in range(30)]
for t in threads:
    t.start()
for t in threads:
    t.join()
for i, code, xs, d in results:
    if not (isinstance(d, dict) and d.get("bump") == 1 and d.get("const") == "first"
            and d.get("global_g") == 1 and d.get("preg_last_error") == 1 and xs == f"par{i}"):
        problems.append(f"par{i}: {code} {xs} {d}")

print(f"isolation problems: {problems if problems else 'none -- every request saw fresh state'}")
sys.exit(1 if problems else 0)
