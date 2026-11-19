--TEST--
Suspend while a finally block is preserving a pending exception
--FILE--
<?php
$fiber = new Fiber(function () {
    try { throw new RuntimeException('raised'); }
    finally { var_dump(Fiber::suspend('finally')); }
});
var_dump($fiber->start());
try { $fiber->resume('resumed'); } catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
$a = new Fiber(function () {
    try { Fiber::suspend('child'); }
    finally { (new Fiber(fn () => print "other fiber\n"))->start(); }
});
$a->start();
echo "done\n";
--EXPECT--
string(7) "finally"
string(7) "resumed"
raised
done
other fiber
