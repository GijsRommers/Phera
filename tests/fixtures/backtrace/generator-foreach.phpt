--TEST--
Iterator-driven generators point at the foreach call site
--FILE--
<?php
function inspect() {
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    yield 1;
}
foreach (inspect() as $item) {}
--EXPECTF--
#0 %s/generator-foreach.php(6): inspect()
