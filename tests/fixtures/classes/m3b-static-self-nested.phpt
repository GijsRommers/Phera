--TEST--
Class members: static-self-nested
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){self::$x['k'][]=3; var_dump(self::$x);}} (new Foo)->run();
--EXPECT--
array(1) {
  ["k"]=>
  array(1) {
    [0]=>
    int(3)
  }
}
