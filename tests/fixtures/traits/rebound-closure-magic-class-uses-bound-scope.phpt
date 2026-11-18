--TEST--
traits: rebound closure magic class uses bound scope
--FILE--
<?php
trait T {
    static function make() { return static function () { return __CLASS__; }; }
}
class A { use T; }
class B {}
$closure = Closure::bind(A::make(), null, B::class);
echo $closure();
--EXPECT--
B
