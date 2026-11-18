#!/usr/bin/env python3
"""Area 2 battery: request parsing and smuggling shapes.

Each case sends raw bytes and prints status + framing headers + whether more
than one response came back on a pipelined connection. Malformed requests
are expected to get 400 + `connection: close` and to poison nothing after
them.
"""
import sys

from lib import build_request, port_arg, send_raw, split_response, status_of

port = port_arg(sys.argv)


def raw_case(label, data, read_timeout=5.0):
    resp = send_raw(port, data, read_timeout=read_timeout)
    code, reason = status_of(resp)
    _, hdrs, body = split_response(resp)
    framing = [(n, v) for n, v in hdrs if n.lower() in ("connection", "content-length", "transfer-encoding")]
    print(f"{label:44s} status={code} {reason:22s} framing={framing} body={body[:40]!r}")


def pipeline(label, requests, read_timeout=5.0):
    resp = send_raw(port, b"".join(requests), read_timeout=read_timeout)
    code, _ = status_of(resp)
    n = resp.count(b"HTTP/1.1 ")
    print(f"{label:44s} first={code} responses={n} bytes={len(resp)}")


H = [("Host", "x")]
raw_case("CL twice same", build_request("POST", "/post.php", H + [("Content-Length", "5"), ("Content-Length", "5"), ("Content-Type", "application/x-www-form-urlencoded")], b"a=1&b"))
raw_case("CL twice conflicting", build_request("POST", "/post.php", H + [("Content-Length", "5"), ("Content-Length", "6")], b"a=1&b"))
raw_case("CL + TE chunked", build_request("POST", "/post.php", H + [("Content-Length", "5"), ("Transfer-Encoding", "chunked")], b"5\r\na=1&b\r\n0\r\n\r\n"))
raw_case("TE chunked twice", build_request("POST", "/post.php", H + [("Transfer-Encoding", "chunked"), ("Transfer-Encoding", "chunked")], b"0\r\n\r\n"))
raw_case("TE identity + CL", build_request("POST", "/post.php", H + [("Transfer-Encoding", "identity"), ("Content-Length", "5")], b"a=1&b"))
raw_case("chunk size not hex", build_request("POST", "/post.php", H + [("Transfer-Encoding", "chunked")], b"ZZ\r\nhi\r\n0\r\n\r\n"))
raw_case("chunk size negative", build_request("POST", "/post.php", H + [("Transfer-Encoding", "chunked")], b"-5\r\nhi\r\n0\r\n\r\n"))
raw_case("CL negative", build_request("POST", "/post.php", H + [("Content-Length", "-1")]))
raw_case("CL huge >u64", build_request("POST", "/post.php", H + [("Content-Length", "99999999999999999999")]))
raw_case("header name with space", b"GET /hello.php HTTP/1.1\r\nHost: x\r\nX Bad: 1\r\n\r\n")
raw_case("header value with NUL", b"GET /hello.php HTTP/1.1\r\nHost: x\r\nX-A: a\x00b\r\n\r\n")
raw_case("obs-fold continuation", b"GET /hello.php HTTP/1.1\r\nHost: x\r\nX-A: 1\r\n  cont\r\n\r\n")
raw_case("bare LF line ends", b"GET /hello.php HTTP/1.1\nHost: x\nConnection: close\n\n")
raw_case("bare CR line ends", b"GET /hello.php HTTP/1.1\rHost: x\r\r")
raw_case("long target 100k", build_request("GET", "/" + "a" * 100000, H))
raw_case("invalid version HTTP/9.9", build_request("GET", "/hello.php", H, version=b"HTTP/9.9"))
raw_case("version HTTP/1.10", build_request("GET", "/hello.php", H, version=b"HTTP/1.10"))
raw_case("unknown method FROB", build_request("FROB", "/echo.php", H))
raw_case("OPTIONS *", b"OPTIONS * HTTP/1.1\r\nHost: x\r\n\r\n")
raw_case("absolute-form target", b"GET http://example.com/hello.php HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
raw_case("200 headers", build_request("GET", "/hello.php", H + [("X-H%d" % i, "v") for i in range(200)]))
raw_case("one 100k header value", build_request("GET", "/hello.php", H + [("X-Big", "v" * 100000)]))
pipeline("pipelined 2 valid", [build_request("GET", "/hello.php", H), build_request("GET", "/sub/data.txt", H)])
pipeline("malformed first then valid", [b"GARBAGE\r\n\r\n", build_request("GET", "/hello.php", H)])
pipeline("bad CL first then valid", [build_request("POST", "/post.php", H + [("Content-Length", "abc")]), build_request("GET", "/hello.php", H)])
