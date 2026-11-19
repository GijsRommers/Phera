--TEST--
getmypid, get_cfg_var, set_time_limit, connection state, ignore_user_abort, escapeshellarg, array_change_key_case, strtok
--FILE--
<?php
var_dump(getmypid() > 0, getmypid() === getmypid(), get_cfg_var('cfg_file_path'), get_cfg_var('nope'), set_time_limit(30), set_time_limit(0), connection_aborted(), connection_status(), CONNECTION_NORMAL, CONNECTION_ABORTED, CONNECTION_TIMEOUT);
var_dump(ignore_user_abort(), ignore_user_abort(true), ignore_user_abort(), ignore_user_abort(false), ignore_user_abort(null), ignore_user_abort());
foreach (["plain", "it's", "", "a b", "new\nline", "x'y'z", '$HOME `id` "q"'] as $s) { echo escapeshellarg($s), "\n"; }
try { escapeshellarg("a\0b"); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
var_dump(array_change_key_case(['One' => 1, 'TWO' => 2, 3 => 'three', 'ÄÖ' => 'umlaut', 'one' => 'later wins']));
var_dump(array_change_key_case(['One' => 1, 'two' => 2, 'ONE' => 'later'], CASE_UPPER), array_change_key_case([]), array_change_key_case(['a' => ['Nested' => 1]], CASE_UPPER));
try { array_change_key_case(['a' => 1], 7); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
// strtok: the classic loop, several delimiters, empty tokens skipped, switching delimiters, restart
$t = strtok("  alpha, beta;gamma ,, delta  ", " ,;"); while ($t !== false) { echo "[$t]"; $t = strtok(" ,;"); } echo "\n";
var_dump(strtok("/api/ping?name=x", "?"), strtok(""), strtok("?"), strtok("?"));
var_dump(strtok("a/b/c", "/"), strtok("/"), strtok("x"), strtok("/"));
var_dump(strtok("", ","), strtok(","), strtok(",,,", ","), strtok("only", ","), strtok(","));
var_dump(strtok("one two", " "), strtok("new string here", " "), strtok(" "), strtok(" "), strtok(" "));
var_dump(strtok("a\0b\0c", "\0"), strtok("\0"), strtok("abc", ""), strtok(""));
function path_only(string $uri): string { return strtok($uri, '?') ?: '/'; }
echo path_only('/hello?x=1'), path_only('?x'), path_only(''), "\n";
try { strtok(); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
var_dump(function_exists('getmypid'), function_exists('strtok'), function_exists('array_change_key_case'), function_exists('escapeshellarg'));
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
bool(true)
int(0)
int(0)
int(0)
int(1)
int(2)
int(0)
int(0)
int(1)
int(1)
int(0)
int(0)
'plain'
'it'\''s'
''
'a b'
'new
line'
'x'\''y'\''z'
'$HOME `id` "q"'
ValueError: escapeshellarg(): Argument #1 ($arg) must not contain any null bytes
array(4) {
  ["one"]=>
  string(10) "later wins"
  ["two"]=>
  int(2)
  [3]=>
  string(5) "three"
  ["ÄÖ"]=>
  string(6) "umlaut"
}
array(2) {
  ["ONE"]=>
  string(5) "later"
  ["TWO"]=>
  int(2)
}
array(0) {
}
array(1) {
  ["A"]=>
  array(1) {
    ["Nested"]=>
    int(1)
  }
}
[alpha][beta][gamma][delta]
string(9) "/api/ping"
string(6) "name=x"
bool(false)
bool(false)
string(1) "a"
string(1) "b"
string(1) "c"
bool(false)
bool(false)
bool(false)
bool(false)
string(4) "only"
bool(false)
string(3) "one"
string(3) "new"
string(6) "string"
string(4) "here"
bool(false)
string(1) "a"
string(1) "b"
string(3) "abc"
bool(false)
/hellox/
ArgumentCountError: strtok() expects at least 1 argument, 0 given
bool(true)
bool(true)
bool(true)
bool(true)
