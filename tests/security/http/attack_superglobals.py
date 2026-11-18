#!/usr/bin/env python3
"""Area 5 battery: superglobal parsing and $_SERVER spoofing.

Dumps $_GET/$_POST/$_COOKIE/$_REQUEST/$_SERVER from /echo.php for a series
of hostile inputs, and checks that security-relevant $_SERVER keys
(REMOTE_ADDR, SCRIPT_FILENAME, DOCUMENT_ROOT, HTTPS, REQUEST_METHOD,
SERVER_ADDR) cannot be set through request headers -- only HTTP_-prefixed
copies may appear (the httpoxy family).
"""
import json
import sys

from lib import build_request, get, port_arg, send_raw, split_response, status_of

port = port_arg(sys.argv)


def dump(target, method="GET", headers=(), body=b""):
    resp = get(port, target, extra_headers=list(headers), body=body, method=method, read_timeout=30)
    code, _ = status_of(resp)
    _, _, raw = split_response(resp)
    i = raw.find(b"{")
    try:
        return code, json.loads(raw[i:].decode("utf-8", "replace"))
    except Exception:
        return code, raw[:120]


QUERIES = [
    "a" + "[x]" * 64 + "=1&b=2",
    "a" + "[x]" * 65 + "=1&b=2",
    "a=1&a[]=2", "a[b]=1&a=2", "a=1&a[b]=2", "a[]=1&a=2",
    "a[x]=1&a[x][y]=2", "a=1&a=2", "5=x&05=y&5.0=z", "a%00b=1&c=2",
    "a=%zz", "a=%", "caf%C3%A9=ok", "a=%ff%fe", "a=1+2%2B3", "&&a=1",
]
for q in QUERIES:
    code, d = dump("/echo.php?" + q)
    g = d.get("GET") if isinstance(d, dict) else d
    print(f"query {q[:40]!r:44s} -> {code} GET={json.dumps(g)[:110]}")

for n in (999, 1001, 20000):
    body = b"&".join(b"k%d=v" % i for i in range(n))
    code, d = dump("/echo.php", method="POST", headers=[("Content-Type", "application/x-www-form-urlencoded")], body=body)
    post = d.get("POST") if isinstance(d, dict) else {}
    print(f"POST {n} params ({len(body)}B) -> {code} parsed={len(post)}")

for ck in ("id=1; id=2; theme=dark", "a[b]=1; a=2"):
    code, d = dump("/echo.php", headers=[("Cookie", ck)])
    print(f"cookie {ck!r:30s} -> COOKIE={json.dumps(d.get('COOKIE'))}")

for ct in ("multipart/form-data; boundary=B", "application/json"):
    code, d = dump("/post.php", method="POST", headers=[("Content-Type", ct)], body=b'{"a":1}' if "json" in ct else b"--B\r\n\r\n1\r\n--B--\r\n")
    print(f"POST Content-Type {ct!r:40s} -> {code} {json.dumps(d)[:110]}")

spoof = [
    ("Proxy", "http://evil:3128"), ("X-Forwarded-For", "1.2.3.4"), ("X-Real-IP", "5.6.7.8"),
    ("HTTPS", "on"), ("Remote-Addr", "9.9.9.9"), ("Script-Filename", "/etc/passwd"),
    ("Document-Root", "/etc"), ("Server-Addr", "8.8.8.8"), ("Request-Method", "DELETE"),
    ("Host", "evil.example"),
]
code, d = dump("/echo.php", headers=spoof)
srv = d.get("SERVER", {}) if isinstance(d, dict) else {}
fixed = {k: srv.get(k) for k in ("REMOTE_ADDR", "SCRIPT_FILENAME", "DOCUMENT_ROOT", "HTTPS", "REQUEST_METHOD", "SERVER_ADDR", "SERVER_NAME")}
print(f"spoof battery -> {code}")
print(f"  fixed keys: {fixed}")
print(f"  HTTP_* copies: { {k: v for k, v in srv.items() if k.startswith('HTTP_') and any(s in k for s in ('PROXY','FORWARD','HTTPS','REMOTE','SCRIPT','DOCUMENT','SERVER_ADDR','REQUEST_METHOD','HOST'))} }")
bad = [k for k, v in fixed.items() if v not in ("127.0.0.1", None) and k in ("REMOTE_ADDR", "SERVER_ADDR", "HTTPS", "REQUEST_METHOD")]
print(f"  SPOOFABLE fixed keys: {bad if bad else 'none'}")

resp = send_raw(port, build_request("GET", "/echo.php", [("Host", "x"), ("X-Raw", b"\xff\xfe\x00abc"), ("Connection", "close")]))
print(f"header value with raw bytes \\xff\\xfe\\x00 -> {status_of(resp)[0]}")
