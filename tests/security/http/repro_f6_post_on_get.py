#!/usr/bin/env python3
"""F6: `$_POST` is filled for any request method whose Content-Type is a
form, not only POST.

PHP populates `$_POST` only when REQUEST_METHOD is POST. Phera's `is_form`
check ignores the method, so `GET /post.php` with a form body yields
$_POST=['a'=>'1']. Applications that treat GET as side-effect-free but read
$_POST diverge from PHP.

Expect (fixed): $_POST empty for non-POST methods, as in PHP.
"""
import json
import sys

from lib import get, port_arg, report, split_response, status_of

port = port_arg(sys.argv)
rc = 0
for method in ("GET", "PUT"):
    resp = get(
        port,
        "/post.php",
        extra_headers=[("Content-Type", "application/x-www-form-urlencoded")],
        body=b"a=1",
        method=method,
    )
    code, _ = status_of(resp)
    _, _, body = split_response(resp)
    try:
        post = json.loads(body.decode())["POST"]
    except Exception:
        post = None
    rc |= report(
        f"{method} with form body", post == {"a": "1"}, f"status={code} $_POST={post!r}"
    )
sys.exit(rc)
