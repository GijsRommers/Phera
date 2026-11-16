--TEST--
Class members: static-static-write
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){static::$x=5; var_dump(static::$x);}} (new Foo)->run();
--EXPECT--
int(5)
