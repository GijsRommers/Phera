#!/usr/bin/env python3
"""Area 1 battery: paths that must not reach files outside the site.

Prints one line per request: status and whether secret content leaked.
Run against a server serving the fixture site from run_all.sh.
"""
import sys

from lib import get, port_arg, split_response, status_of

port = port_arg(sys.argv)

CASES = [
    "/hello.php",
    "/secret.PHP",
    "/secret.pHp",
    "/config.php5",
    "/page.phtml",
    "/.env",
    "/%2e%65%6e%76",
    "/.git/config",
    "/../secret-outside.txt",
    "/%2e%2e/secret-outside.txt",
    "/%252e%252e/secret-outside.txt",
    "/..%2fsecret-outside.txt",
    "/sub/../../secret-outside.txt",
    "/sub/%2e%2e/%2e%2e/secret-outside.txt",
    "/..%5c..%5csecret-outside.txt",
    "//private/tmp/secret-outside.txt",
    "/%2fprivate%2ftmp%2fsecret-outside.txt",
    "/hello.php%00.txt",
    "/hello.php.",
    "/hello.php%20",
    "/hello.php/extra",
    "/hello.php%2fextra",
    "//sub/data.txt",
    "/./sub/data.txt",
    "/%c0%afsecret-outside.txt",
    "/%ff%fe.txt",
    "/sub/%01data.txt",
    "/link.txt",
    "/evil.php",
    "/dirlink/x/secret-outside.txt",
    "/sub/",
    "/../../../../etc/passwd",
    "/%2e%2e/%2e%2e/%2e%2e/%2e%2e/etc/passwd",
    "/.../secret-outside.txt",
    "/%2e/secret-outside.txt",
]

SECRETS = [
    b"SUPERSECRET-PHP-SOURCE", b"KEY-php5", b"topsecret-value-123", b"git-secret",
    b"OUTSIDE-SECRET-98765", b"OUTSIDE-PHP-RAN", b"root:", b"<?php",
]

for target in CASES:
    try:
        resp = get(port, target, read_timeout=8)
    except Exception as e:
        print(f"{target!r:60s} EXCEPTION {e}")
        continue
    code, _ = status_of(resp)
    _, _, body = split_response(resp)
    leaked = [s.decode() for s in SECRETS if s in resp]
    print(f"{target!r:60s} status={code} len={len(body)}" + (f"  LEAK={leaked}" if leaked else ""))
