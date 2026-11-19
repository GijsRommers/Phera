--TEST--
Generator body backtrace when resumed by current
--FILE--
<?php
function generate() {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    echo (new Exception())->getTraceAsString(), "\n";
    yield 1;
}
generate()->current();
--EXPECTF--
#0 [internal function]: generate()
#1 %s/generator-body-current.php(7): Generator->current()
#0 [internal function]: generate()
#1 %s/generator-body-current.php(7): Generator->current()
#2 {main}
