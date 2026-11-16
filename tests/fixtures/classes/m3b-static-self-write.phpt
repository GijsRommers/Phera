--TEST--
Class members: static-self-write
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){self::$x=5; var_dump(self::$x);}} (new Foo)->run();
--EXPECT--
int(5)
