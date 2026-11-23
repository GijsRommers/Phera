#!/usr/bin/env python3
"""Runs every long-source shape through real PHP and through Phera.

For each shape from `generate.php` and each size, it records what both engines
do: the output when they agree, the first line of an error, or how the process
died (a signal such as SIGABRT from a stack overflow, or a timeout), with wall
time and peak memory. The result is a Markdown table on standard output.

    tests/long-sources/table.py --phera target/release/phera
    tests/long-sources/table.py --phera target/debug/phera --sizes 1000,10000 --only add,concat

PHP is `php` on the path unless `--php` says otherwise; it always runs with
`-n`, like every oracle comparison in this repository.
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
# Lines of the `/usr/bin/time` report: "  123  page reclaims", "\tUser time ...".
TIME_REPORT = re.compile(r"^\s+(\d+\s+\S|\S.*:\s)|^\s*\d+\.\d+ real")


def run(command, timeout):
    """Runs one engine; returns (status, output, seconds, peak MB).

    Peak memory comes from `/usr/bin/time` (`-l` on macOS, `-v` on Linux),
    whose report is cut off the end of standard error.
    """
    flag = "-l" if sys.platform == "darwin" else "-v"
    start = time.monotonic()
    try:
        done = subprocess.run(["/usr/bin/time", flag] + command,
                              capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return ("timeout", "", timeout, 0.0)
    seconds = time.monotonic() - start
    stderr = done.stderr.decode("utf-8", "replace")
    peak_mb = 0.0
    kept = []
    for line in stderr.splitlines():
        if "maximum resident set size" in line:
            peak_mb = int(line.split()[0]) / 1e6
        elif "Maximum resident set size" in line:
            peak_mb = int(line.split()[-1]) / 1e3
        elif not TIME_REPORT.match(line):
            kept.append(line)
    text = (done.stdout.decode("utf-8", "replace") + "\n".join(kept)).strip()
    # The temporary directory differs between runs; keep messages comparable.
    # The resolved path first: it can contain the other (/private/var/...).
    for directory in (os.path.dirname(os.path.realpath(command[-1])), os.path.dirname(command[-1])):
        text = text.replace(directory + "/", "")
    returncode = done.returncode
    if returncode > 128 and done.stdout == b"" and "overflowed its stack" in text:
        returncode = -6
    if returncode < 0:
        return (f"signal {-returncode}", text, seconds, peak_mb)
    if returncode in (134, 128 + 6):
        return ("abort", text, seconds, peak_mb)
    return ("exit %d" % returncode, text, seconds, peak_mb)


def describe(result):
    status, text, seconds, peak = result
    first = next((line.strip() for line in text.splitlines() if line.strip()), "")
    shown = first if len(first) <= 70 else first[:67] + "..."
    shown = shown.replace("|", "\\|")
    return f"{status}: `{shown}` ({seconds:.2f} s, {peak:.0f} MB)"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--php", default="php")
    parser.add_argument("--phera", required=True)
    parser.add_argument("--sizes", default="1000,10000,100000,1000000")
    parser.add_argument("--only", default="")
    parser.add_argument("--timeout", type=float, default=60.0)
    arguments = parser.parse_args()

    shapes = subprocess.run(
        [arguments.php, "-n", os.path.join(HERE, "generate.php"), "--list"],
        capture_output=True, check=True, text=True,
    ).stdout.split()
    if arguments.only:
        wanted = arguments.only.split(",")
        shapes = [shape for shape in shapes if shape in wanted]
    sizes = [int(size) for size in arguments.sizes.split(",")]

    print("| shape | size | PHP 8.5 | Phera | same output |")
    print("|---|---|---|---|---|")
    with tempfile.TemporaryDirectory() as directory:
        for shape in shapes:
            for size in sizes:
                path = os.path.join(directory, f"{shape}-{size}.php")
                with open(path, "wb") as source:
                    subprocess.run(
                        [arguments.php, "-n", "-d", "memory_limit=-1",
                         os.path.join(HERE, "generate.php"), shape, str(size)],
                        stdout=source, check=True,
                    )
                php = run([arguments.php, "-n", path], arguments.timeout)
                phera = run([arguments.phera, path], arguments.timeout)
                same = "yes" if php[:2] == phera[:2] else "**no**"
                print(f"| {shape} | {size} | {describe(php)} | {describe(phera)} | {same} |",
                      flush=True)


if __name__ == "__main__":
    main()
