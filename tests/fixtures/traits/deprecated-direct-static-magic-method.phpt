--TEST--
Direct static trait calls through __callStatic warn using the requested method
--FILE--
<?php
trait Behavior {
    public static function __callStatic(string $method, array $arguments): void {
        echo $method, "\n";
    }
}
for ($i = 0; $i < 2; ++$i) { Behavior::MiSSing(); }
--EXPECTF--
Deprecated: Calling static trait method Behavior::MiSSing is deprecated, it should only be called on a class using the trait in %s on line 7
MiSSing

Deprecated: Calling static trait method Behavior::MiSSing is deprecated, it should only be called on a class using the trait in %s on line 7
MiSSing
