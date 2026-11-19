--TEST--
A native frame counts towards the trace limit
--FILE--
<?php
function inspect($value) { echo json_encode(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 1), JSON_UNESCAPED_SLASHES); }
array_map('inspect', [12]);
--EXPECT--
[{"function":"inspect"}]
