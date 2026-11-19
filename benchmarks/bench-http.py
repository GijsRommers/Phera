#!/usr/bin/env python3
"""Measure retired instructions for plain PHP requests on one server worker (macOS)."""

import argparse
import http.client
import os
from pathlib import Path
import re
import selectors
import signal
from statistics import median
import subprocess
import tempfile


def measure(binary, directory, count):
    pid_file = directory / "server.pid"
    pid_file.unlink(missing_ok=True)
    # Keep the timer as the parent so it reports after the server shuts down.
    # The shell records its PID and exec replaces it with the server.
    command = [
        "/usr/bin/time", "-l", "/bin/sh", "-c", 'echo "$$" > "$1"; shift; exec "$@"',
        "sh", pid_file, binary, "serve", "--listen", "127.0.0.1:0", "--workers", "1", directory,
    ]
    with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as timer:
        try:
            with selectors.DefaultSelector() as output:
                output.register(timer.stdout, selectors.EVENT_READ)
                if not output.select(timeout=10):
                    raise RuntimeError(f"{binary} did not announce its listener")
                banner = timer.stdout.readline()
            address = re.search(rb"http://127\.0\.0\.1:(\d+)", banner)
            if not address:
                raise RuntimeError(f"{binary} did not start: {banner!r}")
            connection = http.client.HTTPConnection("127.0.0.1", int(address[1]), timeout=10)
            try:
                for _ in range(count):
                    connection.request("GET", "/index.php")
                    response = connection.getresponse()
                    if response.status != 200 or response.read() != b"ok":
                        raise RuntimeError(f"{binary} returned HTTP {response.status}")
            finally:
                connection.close()
        finally:
            if pid_file.exists():
                server_pid = int(pid_file.read_text())
                try:
                    os.kill(server_pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            try:
                _, metrics = timer.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                if pid_file.exists():
                    os.kill(server_pid, signal.SIGKILL)
                timer.communicate()
                raise
    instructions = re.search(rb"(\d+)\s+instructions retired", metrics)
    if timer.returncode or not instructions:
        raise RuntimeError(metrics.decode(errors="replace"))
    return int(instructions[1])


def positive(text):
    value = int(text)
    if value < 1:
        raise argparse.ArgumentTypeError("expected a positive integer")
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--requests", nargs="+", type=positive, default=[200, 1000])
    parser.add_argument("--rounds", type=positive, default=6)
    options = parser.parse_args()
    binaries = [("candidate", options.candidate.resolve(strict=True))]
    if options.baseline:
        binaries.insert(0, ("baseline", options.baseline.resolve(strict=True)))
    with tempfile.TemporaryDirectory(prefix="phera-bench-http-") as site:
        directory = Path(site)
        (directory / "index.php").write_text('<?php echo "ok";\n')
        for count in options.requests:
            samples = {name: [] for name, _ in binaries}
            for round_number in range(options.rounds):
                order = binaries if round_number % 2 == 0 else reversed(binaries)
                for name, binary in order:
                    samples[name].append(measure(binary, directory, count))
            for name, series in samples.items():
                print(f"{count:,} requests, {name}: {median(series):,.1f} instructions {series}")
            if options.baseline:
                before, after = median(samples["baseline"]), median(samples["candidate"])
                print(f"{count:,} requests: {(after / before - 1) * 100:+.3f}%, "
                      f"{(after - before) / count:+,.1f} instructions/request", flush=True)


if __name__ == "__main__":
    main()
