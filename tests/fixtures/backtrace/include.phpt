--TEST--
Include records the included file argument
--FILE--
<?php
function inspect() { echo json_encode(debug_backtrace(), JSON_UNESCAPED_SLASHES); }
include __DIR__.'/trace-file.inc';
--EXPECTF--
[{"file":"%s/trace-file.inc","line":2,"function":"inspect","args":[]},{"file":"%s/include.php","line":3,"args":["%s/trace-file.inc"],"function":"include"}]
