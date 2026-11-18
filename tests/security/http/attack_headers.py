#!/usr/bin/env python3
"""Area 3 battery: header()/http_response_code() injection attempts from
request data. Every payload is percent-encoded into /hdr.php?h=... which
calls header($_GET['h']).

Expected: CRLF/NUL payloads are refused (PHP refuses them too); no second
header line or second response ever appears. HTAB inside a value is legal
(RFC 9110 field-content) and stays inside the single header value.
Note: a script-set Content-Length is a separate finding (repro_f2).
"""
import sys

from lib import get, port_arg, split_response, status_of

port = port_arg(sys.argv)

PAYLOADS = [
    "X-A: 1\r\nSet-Cookie: pwned=1",
    "X-A: 1\nSet-Cookie: pwned=1",
    "X-A: 1\rSet-Cookie: pwned=1",
    "X-A: 1\x00Set-Cookie: pwned=1",
    "X-A: 1\r\n\r\nHTTP/1.1 200 OK\r\nX-Injected: yes\r\n\r\n<html>",
    "HTTP/1.1 200 OK\r\nX-Injected: yes",
    "X-A: 1\x0bSet-Cookie: pwned=1",
    "X-A: 1\x0cSet-Cookie: pwned=1",
    "X-A: 1\x7fSet-Cookie: pwned=1",
    "X-A: 1\x01Set-Cookie: pwned=1",
    "X-A: 1\tSet-Cookie: pwned=1",
    "X-A\r\n: 1",
    "X-A: 1\r\n",
]


def enc(s):
    out = []
    for ch in s:
        o = ord(ch)
        if ch in ":/% \r\n\x00\x0b\x0c\x7f\x01\t" or o < 32 or o > 126:
            out.append("%%%02X" % o)
        else:
            out.append(ch)
    return "".join(out)


for payload in PAYLOADS:
    resp = get(port, "/hdr.php?h=" + enc(payload), read_timeout=8)
    code, _ = status_of(resp)
    head, hdrs, body = split_response(resp)
    lines = head.split("\r\n")
    injected = len(lines) > 1 and any(
        "pwned" in v or "X-Injected" in n for n, v in hdrs
    )
    print(f"payload={payload!r:60s} status={code} injected={injected} hdrs={hdrs[:2]}")

for target in ["/hdr.php?h=HTTP%2F1.1%20404%20Not%20Found", "/code.php?c=99", "/code.php?c=600", "/code.php?c=65536"]:
    resp = get(port, target, read_timeout=8)
    code, reason = status_of(resp)
    print(f"{target:50s} -> {code} {reason}")
