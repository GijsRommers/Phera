#!/usr/bin/env python3
"""F1: PHP source disclosure through a case-variant extension.

`phera serve` decides "is this a script?" with a case-sensitive comparison
(`extension == "php"`), so `/secret.PHP` is served as a static file
(application/octet-stream) instead of being executed. Real PHP's built-in
server matches the extension case-insensitively and runs the file.

Expect (fixed): 404/403 or the script's output -- never the source.
"""
import sys

from lib import get, port_arg, report, split_response, status_of

port = port_arg(sys.argv)
rc = 0
for target in ("/secret.PHP", "/secret.pHp"):
    resp = get(port, target)
    code, _ = status_of(resp)
    _, hdrs, body = split_response(resp)
    ct = next((v for n, v in hdrs if n.lower() == "content-type"), "")
    leaked = b"SUPERSECRET-PHP-SOURCE" in body
    rc |= report(
        f"GET {target}",
        leaked and code == 200,
        f"status={code} content-type={ct!r} body={body[:80]!r}",
    )
sys.exit(rc)
