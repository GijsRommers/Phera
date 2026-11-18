#!/usr/bin/env python3
"""F7: error output sent to the client contains the server's absolute
filesystem paths (document root, script path) and line numbers.

A request for a PHP file with a syntax error returns a 500 whose body is the
compiler diagnostic, which embeds the full canonical path of the file
(`.../sites/site1/broken.php on line 1`). Runtime warnings and uncaught
exceptions likewise print `in /abs/path/file.php:LINE` plus a stack trace.
Production PHP (display_errors=Off) returns an empty 500; php -S with no
php.ini shows the same text as Phera, so this matches the dev default but
there is no way to turn it off.

Expect (fixed): a generic 500 body; diagnostics only in the server log.
"""
import os
import sys

from lib import get, port_arg, report, split_response, status_of

port = port_arg(sys.argv)
resp = get(port, "/broken.php")
code, _ = status_of(resp)
_, _, body = split_response(resp)
cwd_leaked = os.path.basename(os.getcwd()).encode() in body or b"/broken.php on line" in body
abs_leaked = body.count(b"/") >= 3 and b"broken.php" in body
sys.exit(
    report(
        "GET /broken.php",
        code == 500 and abs_leaked,
        f"status={code} body={body[:160]!r}",
    )
)
