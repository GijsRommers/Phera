--TEST--
A native caller remains visible around a native backtrace callback
--FILE--
<?php
$result = array_map('debug_backtrace', [DEBUG_BACKTRACE_IGNORE_ARGS]);
echo json_encode($result, JSON_UNESCAPED_SLASHES);
--EXPECTF--
[[{"file":"%s/native-only-callback.php","line":2,"function":"array_map"}]]
