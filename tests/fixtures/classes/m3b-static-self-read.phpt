--TEST--
Class members: static-self-read
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){var_dump(self::$x);}} (new Foo)->run();
--EXPECT--
array(0) {
}
