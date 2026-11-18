--TEST--
A parent call uses the receiver class magic method
--FILE--
<?php
class ParentCaller { function __call($n, $a) { echo "parent"; } }
class ChildCaller extends ParentCaller {
    function __call($n, $a) { echo "child ", static::class; }
    function test() { parent::missing(); }
}
(new ChildCaller)->test();
--EXPECT--
child ChildCaller
