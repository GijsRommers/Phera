#!/bin/sh
# PHPStan is deliberately external to this repository. See ../README.md.
set -eu
cd "$(dirname "$0")/../../.."
exec python3 crates/analyzer/tests/compare.py "$@"
