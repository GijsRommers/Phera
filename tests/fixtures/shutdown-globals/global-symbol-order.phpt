--TEST--
Parked global names keep PHP destructor order through fibers and shutdown writes
--FILE--
<?php
class ShutdownItem {
    public function __construct(private string $name) {}
    public function __destruct() { echo $this->name, "\n"; }
}
register_shutdown_function(static fn () => print "shutdown\n");
$third = $first = $second = null;
$first = new ShutdownItem('first');
$GLOBALS['inserted_at_runtime'] = new ShutdownItem('dynamic');
$second = new ShutdownItem('second');
$third = new ShutdownItem('third');
$shared = new ShutdownItem('shared');
$alias = $shared;
$contained = [new ShutdownItem('contained')];
$fiber = new Fiber(static function () {
    $GLOBALS['fiber_symbol'] = new ShutdownItem('fiber symbol');
    Fiber::suspend();
});
$fiber->start();
$fiber->resume();
register_shutdown_function(static function () {
    $GLOBALS['first'] = new ShutdownItem('replacement');
    $GLOBALS['shutdown_symbol'] = new ShutdownItem('shutdown symbol');
    $GLOBALS['inserted_at_runtime'] = new ShutdownItem('second replacement');
});
--EXPECT--
shutdown
first
dynamic
shutdown symbol
fiber symbol
second replacement
second
replacement
third
shared
contained
