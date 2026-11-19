--TEST--
Limit counts suspended delegate frames and skips their arguments
--FILE--
<?php
function inner($value) {
    echo json_encode(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2), JSON_UNESCAPED_SLASHES);
    yield 1;
}
function middle($value) { yield from inner($value); }
function outer($value) { yield from middle($value); }
outer('sample')->rewind();
--EXPECTF--
[{"file":"%s/generator-limit.php","line":6,"function":"inner"},{"file":"%s/generator-limit.php","line":7,"function":"middle"}]
