#!/bin/sh
# Sends many requests at once to `phera serve` and to PHP's built-in server
# and prints how each coped.
#
#   examples/concurrency/loadtest.sh [path/to/phera]
#
# Needs `ab` (ApacheBench, part of macOS and of apache2-utils), `curl` and `php`.
#
# PHP's built-in server stands in for PHP-FPM here: both run a fixed number of
# processes, and a process that waits (for a database, another service, or a
# `usleep`) can serve nobody else meanwhile.
set -eu

PHERA="${1:-target/release/phera}"
SITE="$(cd "$(dirname "$0")" && pwd)"
PORT=8371
ulimit -n 10000 2>/dev/null || true   # each open connection is a file descriptor

# run_ab LABEL REQUESTS AT_ONCE SCRIPT [EXTRA_AB_OPTION]
run_ab() {
    # -r: count a refused or reset connection as a failed request and go on.
    result=$(ab -r ${5:-} -n "$2" -c "$3" "http://127.0.0.1:$PORT/$4" 2>&1 || true)
    seconds=$(printf '%s\n' "$result" | awk '/Time taken for tests/ {print $5}')
    if [ -z "$seconds" ]; then
        printf '%-26s %-9s %5s at once  gave up: %s\n' "$1" "$4" "$3" \
            "$(printf '%s\n' "$result" | tail -1)"
    else
        rate=$(printf '%s\n' "$result" | awk '/Requests per second/ {print $4}')
        failed=$(printf '%s\n' "$result" | awk '/Failed requests/ {print $3}')
        median=$(printf '%s\n' "$result" | awk '/ 50% / {print $2}')
        slowest=$(printf '%s\n' "$result" | awk '/100% / {print $2}')
        printf '%-26s %-9s %5s requests %5s at once %8ss  %8s req/s  median %6s ms  slowest %6s ms  failed %s\n' \
            "$1" "$4" "$2" "$3" "$seconds" "$rate" "$median" "$slowest" "$failed"
    fi
    sleep 3   # let the operating system release the closed connections
}

# with_server LABEL DIVISOR COMMAND...
# Starts a server, runs the four scenarios and stops it. DIVISOR shrinks the
# number of requests for servers that would otherwise take minutes.
with_server() {
    label="$1"; divisor="$2"; shift 2
    "$@" >/dev/null 2>&1 &
    server=$!
    until curl -s -o /dev/null "http://127.0.0.1:$PORT/hello.php"; do sleep 0.2; done
    run_ab "$label" $((2000 / divisor)) $((1000 / divisor)) wait.php
    run_ab "$label" $((2000 / divisor)) $((500 / divisor)) page.php
    run_ab "$label" 200 50 cpu.php
    # -k keeps the 100 connections open. Without it this scenario alone would
    # use 20,000 local ports, more than the operating system hands out.
    run_ab "$label" 20000 100 hello.php -k
    pkill -P "$server" 2>/dev/null || true   # PHP's worker processes
    kill "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
    PORT=$((PORT + 1))   # a fresh port, so a slow shutdown cannot answer for the next server
    echo
}

CORES=$(getconf _NPROCESSORS_ONLN)
with_server "phera, 1 thread" 1 "$PHERA" serve --listen "127.0.0.1:$PORT" --workers 1 "$SITE"
with_server "phera, $CORES threads" 1 "$PHERA" serve --listen "127.0.0.1:$PORT" --workers "$CORES" "$SITE"
with_server "php, 1 process" 20 php -n -S "127.0.0.1:$PORT" -t "$SITE"
PHP_CLI_SERVER_WORKERS=$CORES with_server "php, $CORES processes" 4 php -n -S "127.0.0.1:$PORT" -t "$SITE"
PHP_CLI_SERVER_WORKERS=100 with_server "php, 100 processes" 1 php -n -S "127.0.0.1:$PORT" -t "$SITE"
