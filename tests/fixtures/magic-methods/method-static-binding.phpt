--TEST--
Late static syntax finds an overridden magic method
--FILE--
<?php
class A {
    static function __callStatic($n, $a) { echo "A"; }
    static function run() { static::missing(); }
}
class B extends A { static function __callStatic($n, $a) { echo "B"; } }
B::run();
--EXPECT--
B
