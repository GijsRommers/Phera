--TEST--
Generator body backtrace when resumed by send
--FILE--
<?php
function generate() {
    yield 0;
    debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    echo (new Exception())->getTraceAsString(), "\n";
    yield 1;
}
$g = generate();
$g->current();
$g->send(7);
--EXPECTF--
#0 [internal function]: generate()
#1 %s/generator-body-send.php(10): Generator->send()
#0 [internal function]: generate()
#1 %s/generator-body-send.php(10): Generator->send(7)
#2 {main}
