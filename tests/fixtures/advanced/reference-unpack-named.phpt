--TEST--
reference-unpack-named
--FILE--
<?php function modify(&$first, &...$rest) { $first = 7; foreach ($rest as &$value) $value = 8; } $items = ['first' => 1, 'other' => 2]; modify(...$items); var_dump($items);

--EXPECT--
array(2) {
  ["first"]=>
  int(7)
  ["other"]=>
  int(8)
}
