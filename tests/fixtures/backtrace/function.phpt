--TEST--
A function frame includes its caller and current arguments
--FILE--
<?php
function inspect($value) { echo json_encode(debug_backtrace(), JSON_UNESCAPED_SLASHES); }
inspect('hello');
--EXPECTF--
[{"file":"%s/function.php","line":3,"function":"inspect","args":["hello"]}]
