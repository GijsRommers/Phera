--TEST--
Fixed arrays cover null slots in casts, JSON and independent iterators
--FILE--
<?php
$a = SplFixedArray::fromArray([2 => 'value']);
var_dump((array) $a, json_encode($a));
$it = $a->getIterator();
$it->rewind();
foreach ($a as $key => $value) {
    echo $key, ':', $value, "\n";
}
var_dump($it->key(), $it->current());
--EXPECT--
array(3) {
  [0]=>
  NULL
  [1]=>
  NULL
  [2]=>
  string(5) "value"
}
string(19) "[null,null,"value"]"
0:
1:
2:value
int(0)
NULL
