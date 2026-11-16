--TEST--
named-variadic-typed
--FILE--
<?php function collect(int $first, int ...$rest) { var_dump($first, $rest); } collect(1, 2, extra: '3', rest: 4); collect(...['rest' => '5', 'first' => '6']);

--EXPECT--
int(1)
array(3) {
  [0]=>
  int(2)
  ["extra"]=>
  int(3)
  ["rest"]=>
  int(4)
}
int(6)
array(1) {
  ["rest"]=>
  int(5)
}
