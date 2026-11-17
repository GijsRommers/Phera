#!/usr/bin/env bash
# Takes every php-src test that passes on real PHP and on Phera into
# tests/upstream/, and rewrites tests/upstream/README.md with the result.
#
#   tests/phpt/sweep.sh <php-src checkout> [git ref]
#
# The checkout is only read (`git archive`). The ref defaults to the PHP
# version Phera is compared with. Set SWEEP_DIRS to try other directories and
# PHERA_PHP to name the real PHP binary.
#
# A test is taken only if real PHP passes it under our runner, so tests that
# need a section the runner lacks, or an extension that is not loaded, stay
# out, and so do tests that measure time or need the network. Tests already
# selected are never removed: one that stops passing is a
# regression for `make phpt` to report, not something to drop here.
set -euo pipefail

source_checkout=${1:?usage: tests/phpt/sweep.sh <php-src checkout> [git ref]}
ref=${2:-origin/PHP-8.5.12}
directories=${SWEEP_DIRS:-Zend/tests tests ext/standard/tests ext/json/tests ext/pcre/tests ext/ctype/tests}
php=${PHERA_PHP:-php}
# A support file larger than this (a browscap.ini, a font) is not copied.
largest_support_file=256k

root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/phera-sweep.XXXXXX")
tree=$work/tree
mkdir -p "$tree/upstream" "$tree/phpt" "$tree/fixtures"

echo "exporting $ref from $source_checkout into $work"
# shellcheck disable=SC2086  # the directories are separate arguments
git -C "$source_checkout" archive "$ref" $directories LICENSE | tar -x -C "$tree/upstream"
(cd "$tree/upstream" && find . -name '*.phpt' | sed 's#^\./##' | LC_ALL=C sort) >"$tree/phpt/selected.txt"

# Phera has no memory limit yet, and the runner's timeout stops only the
# process it started. This wrapper stops a test that passes 1.5 GB or 9 s.
cat >"$work/capped.sh" <<'WRAPPER'
#!/bin/sh
"$@" &
pid=$!
(
  ticks=0
  while kill -0 "$pid" 2>/dev/null; do
    resident=$(ps -o rss= -p "$pid" 2>/dev/null)
    ticks=$((ticks + 1))
    if [ "${resident:-0}" -gt 1500000 ] || [ "$ticks" -gt 45 ]; then
      kill -9 "$pid" 2>/dev/null
    fi
    sleep 0.2
  done
) >/dev/null 2>&1 &
wait "$pid"
WRAPPER
chmod +x "$work/capped.sh"

cd "$root"
cargo build --release -p phera-cli
cargo test -p phera-cli --test phpt --no-run

# Runs every exported test with one binary; leaves the names that passed,
# relative to tests/upstream/, in $work/<label>.pass.
run_all() {
  local label=$1
  shift
  echo "running $(wc -l <"$tree/phpt/selected.txt" | tr -d ' ') tests with $label"
  SKIP_ONLINE_TESTS=1 SKIP_SLOW_TESTS=1 SKIP_PERF_SENSITIVE=1 \
    PHERA_PHPT_TESTS_DIR=$tree PHERA_PHPT_TMP_DIR=$work/tmp-$label PHERA_PHPT_BIN="$*" \
    cargo test -q -p phera-cli --test phpt >"$work/$label.log" 2>&1 || true
  grep '^PASS ' "$work/$label.log" | awk '{print $2}' | sed 's#^upstream/##' | LC_ALL=C sort >"$work/$label.pass"
}

run_all php "$php" -n -d fatal_error_backtraces=0 -d display_errors=1
run_all phera "$work/capped.sh" "$root/target/release/phera"
LC_ALL=C comm -12 "$work/php.pass" "$work/phera.pass" >"$work/both.pass"

# Copy the tests, and every small file that is not a test: tests include
# them from their own directory, from a subdirectory or from a sibling, so
# picking by test does not work. There are a few hundred, half a megabyte.
upstream=$root/tests/upstream
while read -r test; do
  mkdir -p "$upstream/$(dirname "$test")"
  cp "$tree/upstream/$test" "$upstream/$test"
done <"$work/both.pass"
(cd "$tree/upstream" && find . -type f ! -name '*.phpt' -size "-$largest_support_file") | while read -r file; do
  mkdir -p "$upstream/$(dirname "$file")"
  cp "$tree/upstream/$file" "$upstream/$file"
done

# The selection: what was selected before, plus what passes now.
selection=$root/tests/phpt/selected.txt
{
  grep '^#' "$selection" || true
  echo
  { grep -v -e '^#' -e '^$' "$selection" || true; cat "$work/both.pass"; } | LC_ALL=C sort -u
} >"$work/selected.txt"
mv "$work/selected.txt" "$selection"

# The scoreboard.
{
  echo "# Tests copied from php-src"
  echo
  echo "These files come from the PHP source tree ($ref) and are covered by the"
  echo "PHP License in \`LICENSE\`. \`tests/phpt/selected.txt\` lists the ones"
  echo "\`make phpt\` runs. Do not edit them; \`tests/phpt/sweep.sh\` copies them"
  echo "again and adds every test that newly passes."
  echo
  echo "Last sweep: real PHP and Phera on every test of each directory."
  echo "\"Usable\" is what real PHP passes under our runner; the rest needs a"
  echo "section the runner does not implement or an extension that is not loaded."
  echo
  echo "| Directory | Tests | Usable | Phera passes | Share of usable |"
  echo "| --- | ---: | ---: | ---: | ---: |"
  for directory in $directories; do
    tests=$(grep -c "^$directory/" "$tree/phpt/selected.txt" || true)
    usable=$(grep -c "^$directory/" "$work/php.pass" || true)
    passing=$(grep -c "^$directory/" "$work/both.pass" || true)
    share=$(awk -v p="$passing" -v u="$usable" 'BEGIN { printf "%d%%", u ? 100 * p / u : 0 }')
    echo "| \`$directory\` | $tests | $usable | $passing | $share |"
  done
} >"$upstream/README.md"

echo "selected $(grep -c -v -e '^#' -e '^$' "$selection") tests; logs are in $work"
echo "now run: make phpt   (and once with PHERA_PHPT_BIN set to real PHP)"
