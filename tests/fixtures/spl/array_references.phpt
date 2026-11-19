--TEST--
Array containers share element references with foreach and indirect writes
--FILE--
<?php
foreach ([new ArrayObject([1]), new ArrayIterator([1])] as $o) {
    foreach ($o as &$value) {
        $value++;
    }
    $ref =& $o[0];
    $ref++;
    var_dump($o[0], $value);
    unset($value, $ref);
    $target = 9;
    $o['other'] =& $target;
    $target = 10;
    $o['missing']['nested'] = 'x';
    $o[0]++;
    var_dump($o->getArrayCopy());
}
--EXPECT--
int(3)
int(3)
array(3) {
  [0]=>
  int(4)
  ["other"]=>
  &int(10)
  ["missing"]=>
  array(1) {
    ["nested"]=>
    string(1) "x"
  }
}
int(3)
int(3)
array(3) {
  [0]=>
  int(4)
  ["other"]=>
  &int(10)
  ["missing"]=>
  array(1) {
    ["nested"]=>
    string(1) "x"
  }
}
