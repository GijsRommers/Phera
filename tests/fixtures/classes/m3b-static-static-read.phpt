--TEST--
Class members: static-static-read
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){var_dump(static::$x);}} (new Foo)->run();
--EXPECT--
array(0) {
}
