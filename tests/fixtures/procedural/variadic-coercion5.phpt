--TEST--
variadic-coercion5
--FILE--
<?php function typed5(int ...$values) { var_dump($values); } typed5(first:'4',second:2.0); typed5(...['one'=>'3','two'=>true]);
--EXPECT--
array(2) {
  ["first"]=>
  int(4)
  ["second"]=>
  int(2)
}
array(2) {
  ["one"]=>
  int(3)
  ["two"]=>
  int(1)
}
