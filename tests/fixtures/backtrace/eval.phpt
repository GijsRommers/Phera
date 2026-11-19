--TEST--
The eval frame has no args
--FILE--
<?php
function inspect() { echo json_encode(debug_backtrace(), JSON_UNESCAPED_SLASHES); }
eval('inspect();');
--EXPECTF--
[{"file":"%s/eval.php(3) : eval()'d code","line":1,"function":"inspect","args":[]},{"file":"%s/eval.php","line":3,"function":"eval"}]
