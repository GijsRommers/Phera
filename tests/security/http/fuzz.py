#!/usr/bin/env python3
"""Area 8: mutation fuzzer over recorded valid requests.

Usage: fuzz.py [port] [seconds] [seed]

Sends mutated copies of valid requests (one connection each, some pipelined),
records status counts and anomalies, and checks server liveness with a
control request every 200 requests. The server must never panic, exit or
stop accepting connections. Run for at least two hours in total, at low
priority, e.g.:

    nice -n 15 python3 fuzz.py 18400 7200 42
"""
import random
import socket
import sys
import time

from lib import DEFAULT_PORT, HOST, build_request, port_arg, status_of

SECONDS = float(sys.argv[2]) if len(sys.argv) > 2 else 60.0
SEED = int(sys.argv[3]) if len(sys.argv) > 3 else 1
port = port_arg(sys.argv)
rng = random.Random(SEED)

TEMPLATES = [
    build_request("GET", "/hello.php", [("Host", "x")]),
    build_request("GET", "/", [("Host", "x")]),
    build_request("GET", "/sub/data.txt", [("Host", "x")]),
    build_request("GET", "/echo.php?a=1&b[]=2&c[x][y]=3", [("Host", "x")]),
    build_request("GET", "/echo.php?" + "&".join("k%d=v" % i for i in range(50)), [("Host", "x")]),
    build_request("POST", "/post.php", [("Host", "x"), ("Content-Type", "application/x-www-form-urlencoded"), ("Content-Length", "7")], b"a=1&b=2"),
    build_request("POST", "/post.php", [("Host", "x"), ("Content-Type", "application/x-www-form-urlencoded"), ("Transfer-Encoding", "chunked")], b"7\r\na=1&b=2\r\n0\r\n\r\n"),
    build_request("GET", "/hdr.php?h=X-A%3a%201", [("Host", "x"), ("Cookie", "id=1; theme=dark")]),
    build_request("GET", "/state.php?set=abc", [("Host", "x"), ("X-Forwarded-For", "1.2.3.4"), ("Proxy", "http://evil")]),
    build_request("GET", "/cl.php", [("Host", "x")]) + build_request("GET", "/hello.php", [("Host", "x")]),
    build_request("OPTIONS", "*", [("Host", "x")]),
    b"GET http://example.com/hello.php HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    build_request("GET", "/secret.PHP", [("Host", "x")]),
    build_request("GET", "/%2e%2e/%2e%2e/etc/passwd", [("Host", "x")]),
    build_request("GET", "/a%00b.php", [("Host", "x")]),
    build_request("POST", "/echo.php", [("Host", "x"), ("Content-Type", "multipart/form-data; boundary=B"), ("Content-Length", "60")], b"--B\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n1\r\n--B--\r\n"),
    build_request("GET", "/hello.php", [("Host", "x")], version=b"HTTP/1.0"),
]

INTERESTING = bytes(b"\r\n\x00%[]:?&;= \t/.-_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")


def mutate(data):
    data = bytearray(data)
    for _ in range(rng.randint(1, 8)):
        if not data:
            data += b"GET / HTTP/1.1\r\n\r\n"
        op = rng.randint(0, 9)
        pos = rng.randrange(len(data))
        if op == 0:
            data[pos] = rng.randrange(256)
        elif op == 1:
            data.insert(pos, rng.randrange(256))
        elif op == 2:
            data.insert(pos, rng.choice(INTERESTING))
        elif op == 3:
            del data[pos]
        elif op == 4:
            del data[pos:pos + rng.randint(1, 16)]
        elif op == 5:
            n = rng.randint(1, 32)
            data[pos:pos] = data[pos:pos + n]
        elif op == 6:
            data = data[:pos]
        elif op == 7:
            data += bytes(rng.randrange(256) for _ in range(rng.randint(1, 64)))
        elif op == 8:
            b = data[pos]
            if 65 <= b <= 90:
                data[pos] = b + 32
            elif 97 <= b <= 122:
                data[pos] = b - 32
        else:
            t = rng.choice(TEMPLATES)
            a = rng.randrange(len(t))
            data[pos:pos] = t[a:a + rng.randint(1, 40)]
    return bytes(data)


def send(data, read_timeout):
    try:
        s = socket.create_connection((HOST, port), timeout=read_timeout)
    except OSError as e:
        return None, f"connect-fail {type(e).__name__}"
    try:
        s.settimeout(read_timeout)
        s.sendall(data)
        out = b""
        try:
            while len(out) < 65536:
                c = s.recv(65536)
                if not c:
                    break
                out += c
        except socket.timeout:
            pass
        if not out:
            return None, "empty-response"
        code, _ = status_of(out)
        return (code if code is not None else None), (None if code is not None else "unparseable")
    except OSError as e:
        return None, type(e).__name__
    finally:
        try:
            s.close()
        except OSError:
            pass


def alive():
    try:
        code, _ = send(
            build_request("GET", "/hello.php", [("Host", "x"), ("Connection", "close")]), 5
        )
        return code == 200
    except Exception:
        return False


def main():
    t0 = time.time()
    sent, statuses, anomalies = 0, {}, {}
    while time.time() - t0 < SECONDS:
        tmpl = rng.choice(TEMPLATES)
        data = mutate(tmpl) if rng.random() < 0.85 else tmpl
        code, anomaly = send(data, rng.choice([0.3, 0.5, 1.0]))
        sent += 1
        if code is None:
            anomalies[anomaly] = anomalies.get(anomaly, 0) + 1
        else:
            statuses[code] = statuses.get(code, 0) + 1
        if sent % 200 == 0 and not alive():
            print(f"ALERT: server not answering after {sent} requests", flush=True)
            return 1
    print(
        f"seed={SEED} seconds={SECONDS:.0f} sent={sent} "
        f"statuses={dict(sorted(statuses.items()))} anomalies={anomalies} alive={alive()}",
        flush=True,
    )
    return 0


sys.exit(main())
