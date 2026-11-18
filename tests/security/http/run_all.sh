#!/bin/bash
# run_all.sh -- build a fixture site, start `phera serve` on 127.0.0.1:18400,
# run every reproducer and battery, then stop the server.
#
# Usage: tests/security/http/run_all.sh [path-to-phera-binary] [port]
# The binary defaults to target/release/phera in this repository.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
BIN=${1:-$REPO/target/release/phera}
PORT=${2:-18400}
SITE=$(mktemp -d /tmp/phera-security-site-XXXXXX)
OUTSIDE=$(mktemp -d /tmp/phera-security-outside-XXXXXX)

cleanup() {
  [ -n "${SRV:-}" ] && kill "$SRV" 2>/dev/null
  [ -n "${SRV:-}" ] && wait "$SRV" 2>/dev/null
  rm -rf "$SITE" "$OUTSIDE"
}
trap cleanup EXIT

# ---- fixture site ----
mkdir -p "$SITE/sub" "$SITE/.git"
cat > "$SITE/index.php" <<'EOF'
<?php echo 'FRONT uri=', $_SERVER['REQUEST_URI'], ' method=', $_SERVER['REQUEST_METHOD'];
EOF
printf '<?php echo "hello ", PHP_VERSION;' > "$SITE/hello.php"
printf '<?php $db_password = "SUPERSECRET-PHP-SOURCE"; echo "should never run";\n' > "$SITE/secret.PHP"
printf '<?php $api_key = "KEY-php5";\n' > "$SITE/config.php5"
printf '<?php $phtml = "phtml-source";\n' > "$SITE/page.phtml"
printf 'SECRET=topsecret-value-123\n' > "$SITE/.env"
printf '[core]\n\tsecret = git-secret\n' > "$SITE/.git/config"
printf '<?php echo "sub index";' > "$SITE/sub/index.php"
printf 'plain sub data\n' > "$SITE/sub/data.txt"
printf '<?php echo "unclosed\n' > "$SITE/broken.php"
printf '<?php throw new Exception("boom");\n' > "$SITE/fatal.php"
printf '<?php echo "before"; $x = null; $x->m();' > "$SITE/crash.php"
cat > "$SITE/hdr.php" <<'EOF'
<?php header($_GET['h'] ?? 'X-Default: ok'); echo "body\n";
EOF
cat > "$SITE/code.php" <<'EOF'
<?php http_response_code((int)($_GET['c'] ?? 200)); echo "ok\n";
EOF
cat > "$SITE/cl.php" <<'EOF'
<?php header('Content-Length: 5'); echo str_repeat('A', 100), "\n";
EOF
cat > "$SITE/echo.php" <<'EOF'
<?php echo json_encode(['GET'=>$_GET,'POST'=>$_POST,'COOKIE'=>$_COOKIE,'REQUEST'=>$_REQUEST,'SERVER'=>$_SERVER]);
EOF
cat > "$SITE/post.php" <<'EOF'
<?php echo json_encode(['M'=>$_SERVER['REQUEST_METHOD'],'CT'=>$_SERVER['CONTENT_TYPE'] ?? null,'POST'=>$_POST,'FILES'=>$_FILES]);
EOF
cat > "$SITE/state.php" <<'EOF'
<?php
function bump() { static $c = 0; return ++$c; }
$r = [];
$r['bump'] = bump();
if (!defined('MYCONST')) { define('MYCONST', 'first'); }
$r['const'] = MYCONST;
if (!function_exists('myfunc')) { function myfunc() { return 'v1'; } }
$r['func'] = myfunc();
$GLOBALS['g'] = ($GLOBALS['g'] ?? 0) + 1;
$r['global_g'] = $GLOBALS['g'];
preg_match('/(/', 'x'); $r['preg_last_error'] = preg_last_error();
$r['headers_list'] = headers_list();
header('X-State: ' . ($_GET['set'] ?? 'unset'));
echo json_encode($r);
EOF
printf '<?php $GLOBALS["g"] = 99; header("X-State: exit-set"); exit("bye");\n' > "$SITE/exit.php"
cat > "$SITE/wait.php" <<'EOF'
<?php usleep(300000); echo 'waited ', $_GET['id'] ?? '';
EOF
cat > "$SITE/big.php" <<'EOF'
<?php echo str_repeat('B', 20 * 1024 * 1024);
EOF
cat > "$SITE/grow_bounded.php" <<'EOF'
<?php
// Allocates ~400 MB (102400 x 4 KB) and stops, so the reproducer is safe to
// run anywhere. With no server-side memory limit an unbounded loop reaches
// the kernel OOM killer and takes the whole process down.
$a = [];
for ($i = 0; $i < 102400; $i++) { $a[] = str_repeat('x', 4096); }
echo 'grew to ', count($a), " elements\n";
EOF
head -c 52428800 /dev/zero > "$SITE/big.bin"   # 50 MB static file
printf 'OUTSIDE-SECRET-98765\n' > "$OUTSIDE/secret-outside.txt"
printf '<?php echo "OUTSIDE-PHP-RAN";\n' > "$OUTSIDE/outside.php"
ln -sf "$OUTSIDE/secret-outside.txt" "$SITE/link.txt"
ln -sf "$OUTSIDE/outside.php" "$SITE/evil.php"
ln -sfn "$OUTSIDE" "$SITE/dirlink"

# ---- server ----
nice -n 15 "$BIN" serve --listen 127.0.0.1:$PORT --workers 1 "$SITE" > "$SITE.server.log" 2>&1 &
SRV=$!
for i in $(seq 1 100); do
  (exec 3<>/dev/tcp/127.0.0.1/$PORT) 2>/dev/null && { exec 3>&- 3<&-; break; }
  kill -0 $SRV 2>/dev/null || { echo "server failed to start:"; cat "$SITE.server.log"; exit 1; }
  sleep 0.1
done
export PHERA_PID=$SRV PHERA_TEST_PORT=$PORT

echo "== reproducers (exit 1 = vulnerability present) =="
rc=0
for f in repro_f1_source_disclosure repro_f2_content_length_framing repro_f3_no_memory_limit \
         repro_f4_no_body_timeout repro_f5_buffered_response_pinning repro_f6_post_on_get repro_f7_error_path_leak; do
  echo "--- $f"
  python3 "$HERE/$f.py" "$PORT" || rc=1
done

echo
echo "== attack batteries (informational) =="
for f in attack_paths attack_parsing attack_headers attack_superglobals attack_isolation attack_resources; do
  echo "--- $f"
  timeout 120 python3 "$HERE/$f.py" "$PORT" || true
done

echo
echo "== server log (panics would show here) =="
grep -i "panic" "$SITE.server.log" && rc=1 || echo "no panics in server log"

echo
if [ $rc -eq 0 ]; then echo "RESULT: no reproducer reported a vulnerability"; else echo "RESULT: at least one vulnerability reproduced (see above)"; fi
exit $rc
