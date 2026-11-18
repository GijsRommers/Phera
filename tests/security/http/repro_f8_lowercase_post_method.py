#!/usr/bin/env python3
"""F8: the F6 fix matches the method case-insensitively, so a non-POST method
token in another case (`post`, `Post`) still fills `$_POST` and `$_REQUEST`.

RFC 9110 section 9.1: method names are case-sensitive, and `post` is not the
registered POST method. Real PHP matches the method with a case-sensitive
comparison, so only exactly `POST` fills `$_POST` (verified against
`/opt/homebrew/bin/php -n -S`: a `post` request gets an empty `$_POST`).

Expect (matching PHP): `$_POST` empty for every method other than exactly
`POST`.
"""
import json
import sys

from lib import build_request, port_arg, report, send_raw, split_response, status_of

port = port_arg(sys.argv)
rc = 0
for method in (b"post", b"Post", b"pOsT"):
    resp = send_raw(
        port,
        build_request(
            method,
            "/post.php",
            [("Host", "x"), ("Connection", "close"),
             ("Content-Type", "application/x-www-form-urlencoded")],
            b"a=1",
        ),
    )
    code, _ = status_of(resp)
    _, _, body = split_response(resp)
    try:
        data = json.loads(body.decode())
        post = data["POST"]
        request = data["REQUEST"]
    except Exception:
        post = request = None
    rc |= report(
        f"{method.decode()} with form body",
        post == {"a": "1"},
        f"status={code} REQUEST_METHOD={data['M'] if data else None!r} "
        f"$_POST={post!r} $_REQUEST={request!r}",
    )
sys.exit(rc)
