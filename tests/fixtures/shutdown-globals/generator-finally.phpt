--TEST--
A globally held generator force-closes after shutdown callbacks
--FILE--
<?php
function numbers() {
    try {
        echo "suspended\n";
        yield 1;
    } finally {
        echo "closing\n";
        yield 2;
    }
}
$sequence = numbers();
$sequence->rewind();
register_shutdown_function(static function () {
    global $sequence;
    echo "shutdown ", $sequence->current(), "\n";
});
--EXPECTF--
suspended
shutdown 1
closing

Fatal error: Uncaught Error: Cannot yield from finally in a force-closed generator in %s:8
Stack trace:
#0 [internal function]: numbers()
#1 {main}
  thrown in %s on line 8
