#!/usr/bin/env python3
"""Area 4 battery (bounded): slow headers, declared-huge bodies, idle
keep-alive connections, simultaneous connections, and a client that stops
reading. Stays within the campaign limits: one client process, < 60 s per
flood, < 2000 connections, and it never touches a server it did not start
(see run_all.sh).
"""
import socket
import sys
import threading
import time

from lib import build_request, connect, get, port_arg, status_of

port = port_arg(sys.argv)


def control(tag):
    t0 = time.time()
    try:
        r = get(port, "/hello.php", read_timeout=10)
        return f"{tag}: {status_of(r)[0]} in {time.time()-t0:.2f}s"
    except Exception as e:
        return f"{tag}: ERR {type(e).__name__}"


print(control("baseline"))

# slow headers: 1 byte / 4 s; the 30 s header timeout should cut it off
head = b"GET /hello.php HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
s = connect(port, timeout=80)
t0 = time.time()
result = "still open"
for i in range(len(head)):
    s.sendall(head[i:i + 1])
    s.settimeout(0.3)
    try:
        if s.recv(100):
            result = f"answered at {time.time()-t0:.0f}s"
            break
    except socket.timeout:
        pass
    if time.time() - t0 > 40:
        result = f"still open at {time.time()-t0:.0f}s (no header timeout reset)"
        break
    time.sleep(4)
s.close()
print(f"slow headers 1B/4s: {result}")

# declared huge body, nothing sent
for cl in (1000, 8 * 1024 * 1024, 100 * 1024 * 1024, 2 ** 63):
    s = connect(port, timeout=8)
    s.sendall(b"POST /post.php HTTP/1.1\r\nHost: x\r\nContent-Length: %d\r\n\r\n" % cl)
    s.settimeout(8)
    try:
        r = s.recv(100)
        out = "answered/closed" if not r else "answered"
    except socket.timeout:
        out = "held open, no response (no body timeout)"
    s.close()
    print(f"CL={cl} no body: {out}")

# 300 idle keep-alive connections held 20 s
socks = []
for i in range(300):
    try:
        c = connect(port, timeout=5)
        c.sendall(build_request("GET", "/hello.php", [("Host", "x")]))
        socks.append(c)
    except OSError:
        break
time.sleep(20)
alive = 0
for c in socks[:50]:
    try:
        c.settimeout(2)
        c.sendall(build_request("GET", "/sub/data.txt", [("Host", "x")]))
        if c.recv(200):
            alive += 1
    except OSError:
        pass
print(f"300 idle keep-alive conns held 20s: {alive}/50 still usable (no idle timeout); {control('meanwhile')}")
for c in socks:
    try:
        c.close()
    except OSError:
        pass

# 500 simultaneous requests
results = []
lock = threading.Lock()


def worker(i):
    try:
        r = get(port, "/wait.php?id=%d" % i, read_timeout=30)
        with lock:
            results.append(status_of(r)[0])
    except Exception as e:
        with lock:
            results.append(type(e).__name__)


t0 = time.time()
ts = [threading.Thread(target=worker, args=(i,)) for i in range(500)]
for t in ts:
    t.start()
for t in ts:
    t.join()
ok = sum(1 for c in results if c == 200)
print(f"500 simultaneous requests: {ok}/500 got 200 in {time.time()-t0:.1f}s; other={set(c for c in results if c != 200)}")
