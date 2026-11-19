--TEST--
Generator body backtrace when resumed by foreach
--FILE--
<?php
function generate() {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    echo (new Exception())->getTraceAsString(), "\n";
    yield 1;
}
foreach (generate() as $value) { break; }
--EXPECTF--
#0 %s/generator-body-foreach.php(7): generate()
#0 %s/generator-body-foreach.php(7): generate()
#1 {main}
