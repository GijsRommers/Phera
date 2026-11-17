--TEST--
Class members: static-parent-write
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){parent::$x=5; var_dump(parent::$x);}} (new Foo)->run();
--EXPECT--
int(5)
