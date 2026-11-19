"""Shared helpers for the round-3 security reproducers.

Stdlib only. Every reproducer starts its own fresh `phera serve` on
127.0.0.1, on a port in 18400-18499, with `--workers 1`, at nice 15, with a
fixture site in a private temp directory, and stops it when done. Nothing
outside 127.0.0.1 and the system temporary directory is touched.
"""
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
BIN = os.environ.get("PHERA_BIN", os.path.join(REPO, "target", "release", "phera"))
HOST = "127.0.0.1"
PORT_RANGE = range(18400, 18500)

sys.path.insert(0, HERE)
from lib import build_request, send_raw, split_response, status_of  # noqa: E402


def free_port():
    for port in PORT_RANGE:
        try:
            with socket.socket() as s:
                s.bind((HOST, port))
                return port
        except OSError:
            continue
    raise RuntimeError("no free port in 18400-18499")


def port_arg(argv):
    return int(argv[1]) if len(argv) > 1 else free_port()


def make_site(files):
    """Creates a fixture site in a private temp dir from {name: bytes}."""
    site = tempfile.mkdtemp(prefix="phera-r3-site-")
    for name, content in files.items():
        path = os.path.join(site, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        mode = "wb" if isinstance(content, bytes) else "w"
        with open(path, mode) as f:
            f.write(content)
    return site


class Server:
    """A freshly started `phera serve` on 127.0.0.1, one worker, nice 15."""

    def __init__(self, site, args=(), env_extra=None, cwd=None):
        self.port = free_port()
        self.log = os.path.join(tempfile.gettempdir(), f"phera-r3-server-{self.port}.log")
        env = dict(os.environ)
        if env_extra:
            env.update(env_extra)
        cmd = [
            "nice", "-n", "15", BIN, "serve",
            "--listen", f"{HOST}:{self.port}",
            "--workers", "1",
            *args,
            site,
        ]
        self.proc = subprocess.Popen(
            cmd, stdout=open(self.log, "wb"), stderr=subprocess.STDOUT,
            env=env, cwd=cwd or site, preexec_fn=lambda: os.nice(15),
        )
        for _ in range(200):
            try:
                with socket.create_connection((HOST, self.port), timeout=0.5):
                    break
            except OSError:
                if self.proc.poll() is not None:
                    raise RuntimeError(f"server failed to start; log: {open(self.log).read()[:2000]}")
                time.sleep(0.05)
        else:
            raise RuntimeError("server did not start listening in time")

    def request(self, target, extra_headers=(), body=b"", method="GET", read_timeout=30.0):
        headers = [("Host", "x"), ("Connection", "close"), *extra_headers]
        return send_raw(self.port, build_request(method, target, headers, body), read_timeout)

    def raw(self, data, read_timeout=30.0):
        return send_raw(self.port, data, read_timeout)

    def rss_kb(self):
        out = subprocess.run(["ps", "-o", "rss=", "-p", str(self.proc.pid)],
                             capture_output=True, text=True, timeout=5)
        try:
            return int(out.stdout.strip())
        except ValueError:
            return 0

    def cpu_seconds(self):
        """Cumulative CPU time of the server process, in seconds."""
        out = subprocess.run(["ps", "-o", "time=", "-p", str(self.proc.pid)],
                             capture_output=True, text=True, timeout=5)
        text = out.stdout.strip()  # [[DD-]hh:]mm:ss[.cc]
        if not text:
            return 0.0
        parts = text.split(":")
        try:
            if len(parts) == 3:
                return int(parts[0]) * 3600 + int(parts[1]) * 60 + float(parts[2])
            if len(parts) == 2:
                return int(parts[0]) * 60 + float(parts[1])
            return float(parts[0])
        except ValueError:
            return 0.0

    def log_text(self):
        try:
            return open(self.log, errors="replace").read()
        except OSError:
            return ""

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        try:
            os.unlink(self.log)
        except OSError:
            pass


def report(name, vulnerable, evidence):
    print(f"[{'VULNERABLE' if vulnerable else 'ok'}] {name}")
    print(f"       {evidence}")
    return 1 if vulnerable else 0
