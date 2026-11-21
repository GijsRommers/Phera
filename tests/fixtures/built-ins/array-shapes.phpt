--TEST--
Array slicing, SPL seeking and shallow merge preserve live entry order
--FILE--
<?php
function inspect($array) {
    var_dump(array_slice($array, -2, 1, true), array_slice($array, 1, 2));
    $iterator = new ArrayIterator($array);
    $iterator->seek(1);
    var_dump($iterator->key(), $iterator->current());
    try { $iterator->seek(count($array)); }
    catch (OutOfBoundsException $error) { echo $error->getMessage(), "\n"; }
    var_dump($iterator->valid());
}

$list = ['a', 'b', 'c', 'd'];
$map = [9 => 'z', 'x' => 1, -7 => 2, '03' => 'still a string'];
inspect($list);
inspect($map);
unset($list[1], $map['x']);
inspect($list);
inspect($map);
$reference = 5;
$first = [5 => 'five', 'k' => 'keep', 'ref' => &$reference];
$second = [8 => 'eight', 'k' => 'overwrite'];
$third = [-6 => 'negative', '05' => 'string-key'];
$combined = array_merge($first, $second, $third);
$combined['ref'] = 7;
$combined[] = 'next';
var_dump($combined, $reference, $first);
--EXPECT--
array(1) {
  [2]=>
  string(1) "c"
}
array(2) {
  [0]=>
  string(1) "b"
  [1]=>
  string(1) "c"
}
int(1)
string(1) "b"
Seek position 4 is out of range
bool(false)
array(1) {
  [-7]=>
  int(2)
}
array(2) {
  ["x"]=>
  int(1)
  [0]=>
  int(2)
}
string(1) "x"
int(1)
Seek position 4 is out of range
bool(false)
array(1) {
  [2]=>
  string(1) "c"
}
array(2) {
  [0]=>
  string(1) "c"
  [1]=>
  string(1) "d"
}
int(2)
string(1) "c"
Seek position 3 is out of range
bool(false)
array(1) {
  [-7]=>
  int(2)
}
array(2) {
  [0]=>
  int(2)
  ["03"]=>
  string(14) "still a string"
}
int(-7)
int(2)
Seek position 3 is out of range
bool(false)
array(7) {
  [0]=>
  string(4) "five"
  ["k"]=>
  string(9) "overwrite"
  ["ref"]=>
  &int(7)
  [1]=>
  string(5) "eight"
  [2]=>
  string(8) "negative"
  ["05"]=>
  string(10) "string-key"
  [3]=>
  string(4) "next"
}
int(7)
array(3) {
  [5]=>
  string(4) "five"
  ["k"]=>
  string(4) "keep"
  ["ref"]=>
  &int(7)
}
