--TEST--
Class members: constant-array
--FILE--
<?php class Foo {const A=[1,2];} var_dump(Foo::A);
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
