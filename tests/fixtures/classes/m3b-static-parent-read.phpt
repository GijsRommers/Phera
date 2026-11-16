--TEST--
Class members: static-parent-read
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){var_dump(parent::$x);}} (new Foo)->run();
--EXPECT--
array(0) {
}
