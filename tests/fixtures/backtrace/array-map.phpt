--TEST--
A native callback frame and its PHP call site are distinct
--FILE--
<?php
function inspect($value) { echo json_encode(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS), JSON_UNESCAPED_SLASHES); }
array_map('inspect', [12]);
--EXPECTF--
[{"function":"inspect"},{"file":"%s/array-map.php","line":3,"function":"array_map"}]
