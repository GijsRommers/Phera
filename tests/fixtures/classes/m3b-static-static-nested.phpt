--TEST--
Class members: static-static-nested
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){static::$x['k'][]=3; var_dump(static::$x);}} (new Foo)->run();
--EXPECT--
array(1) {
  ["k"]=>
  array(1) {
    [0]=>
    int(3)
  }
}
