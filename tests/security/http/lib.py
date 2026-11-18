"""Raw-socket HTTP helpers shared by the security reproducers.

Stdlib only; every byte on the wire is explicit. Target only 127.0.0.1.
"""
import os
import socket
import sys
import time

HOST = "127.0.0.1"
DEFAULT_PORT = int(os.environ.get("PHERA_TEST_PORT", "18400"))


def port_arg(argv):
    return int(argv[1]) if len(argv) > 1 else DEFAULT_PORT


def connect(port, timeout=10.0):
    s = socket.create_connection((HOST, port), timeout=timeout)
    s.settimeout(timeout)
    return s


def send_raw(port, data, read_timeout=10.0, max_read=1 << 20):
    s = connect(port, timeout=read_timeout)
    try:
        s.sendall(data)
        out = b""
        deadline = time.time() + read_timeout
        while time.time() < deadline and len(out) < max_read:
            try:
                chunk = s.recv(65536)
            except socket.timeout:
                break
            if not chunk:
                break
            out += chunk
        return out
    finally:
        s.close()


def build_request(method, target, headers, body=b"", version=b"HTTP/1.1"):
    if isinstance(body, str):
        body = body.encode()
    lines = [
        b"%s %s %s"
        % (
            method if isinstance(method, bytes) else method.encode(),
            target if isinstance(target, bytes) else target.encode(),
            version,
        )
    ]
    for name, value in headers:
        n = name if isinstance(name, bytes) else name.encode()
        v = value if isinstance(value, bytes) else value.encode()
        lines.append(n + b": " + v)
    if body and not any(
        k.lower() in ("content-length", "transfer-encoding") for k, _ in headers
    ):
        lines.append(b"Content-Length: " + str(len(body)).encode())
    return b"\r\n".join(lines) + b"\r\n\r\n" + body


def get(port, target, extra_headers=(), body=b"", method="GET", read_timeout=10.0):
    headers = [("Host", f"{HOST}:{port}"), ("Connection", "close")]
    headers.extend(extra_headers)
    return send_raw(port, build_request(method, target, headers, body), read_timeout)


def status_of(resp):
    if not resp:
        return None, ""
    first = resp.split(b"\r\n", 1)[0].split(b"\n", 1)[0]
    parts = first.split(b" ", 2)
    try:
        code = int(parts[1]) if len(parts) >= 2 else None
    except ValueError:
        code = None
    return code, (parts[2].decode("latin1") if len(parts) > 2 else "")


def split_response(resp):
    if b"\r\n\r\n" in resp:
        head, body = resp.split(b"\r\n\r\n", 1)
    else:
        head, body = resp, b""
    lines = head.split(b"\r\n")
    hdrs = []
    for line in lines[1:]:
        if b":" in line:
            n, v = line.split(b":", 1)
            hdrs.append((n.strip().decode("latin1"), v.strip().decode("latin1")))
    return (lines[0].decode("latin1") if lines else ""), hdrs, body


def server_rss_kb(pid):
    """Resident set of a pid in KB, or None if it cannot be read."""
    if not pid:
        return None
    try:
        import subprocess

        out = subprocess.run(
            ["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True, timeout=5
        )
        return int(out.stdout.strip())
    except Exception:
        return None


def report(name, vulnerable, evidence):
    print(f"[{'VULNERABLE' if vulnerable else 'ok'}] {name}")
    print(f"       {evidence}")
    return 1 if vulnerable else 0
