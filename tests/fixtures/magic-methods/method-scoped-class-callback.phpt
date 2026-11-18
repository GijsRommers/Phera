--TEST--
A class callback can use the current instance __call
--FILE--
<?php
class Test {
    function __call($name, $args) { echo $name; }
    function run() { call_user_func([self::class, "unknown"]); }
}
(new Test)->run();
--EXPECT--
unknown
