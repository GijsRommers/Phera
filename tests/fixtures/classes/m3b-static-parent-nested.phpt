--TEST--
Class members: static-parent-nested
--FILE--
<?php class Root { public static $x=[]; } class Foo extends Root {function run(){parent::$x['k'][]=3; var_dump(parent::$x);}} (new Foo)->run();
--EXPECT--
array(1) {
  ["k"]=>
  array(1) {
    [0]=>
    int(3)
  }
}
