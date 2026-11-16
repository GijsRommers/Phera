--TEST--
extract-reference-prefix
--FILE--
<?php
$items = ['a' => 1, 'b' => 2];
extract($items, EXTR_REFS | EXTR_PREFIX_ALL, 'p');
$p_a = 8; $items['b'] = 9; var_dump($items, $p_b);
$copy = $items; extract($items, EXTR_SKIP); $a = 22; var_dump($items, $copy);

--EXPECT--
array(2) {
  ["a"]=>
  &int(8)
  ["b"]=>
  &int(9)
}
int(9)
array(2) {
  ["a"]=>
  &int(8)
  ["b"]=>
  &int(9)
}
array(2) {
  ["a"]=>
  &int(8)
  ["b"]=>
  &int(9)
}
