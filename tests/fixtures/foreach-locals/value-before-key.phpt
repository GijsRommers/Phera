--TEST--
A key that is a plain variable, with a value that is not (list(), a property, an element): PHP still writes the value first, so the value's own targets see the key's old value.
--FILE--
<?php
// A key that is a plain variable, with a value that is not (list(), a
// property, an element): PHP still writes the value first, so the value's
// own targets see the key's old value.
final class Noisy { public function __construct(public string $n) {} public function __destruct() { echo "destruct {$this->n}\n"; } }
function destructure() {
    $k = 'old';
    $seen = [];
    foreach (['a' => [1, 2], 'b' => [3, 4]] as $k => [$seen[$k], $second]) { echo "$k $second\n"; }
    var_dump($seen);
    $k = new Noisy('old key');
    $box = new stdClass;
    foreach (['x' => 1] as $k => $box->value) { echo "body $k\n"; }
    $values = [];
    $k = 'before';
    foreach (['p' => 7, 'q' => 8] as $k => $values[$k]) {}
    var_dump($values);
}
destructure();
--EXPECT--
a 2
b 4
array(2) {
  ["old"]=>
  int(1)
  ["a"]=>
  int(3)
}
destruct old key
body x
array(2) {
  ["before"]=>
  int(7)
  ["p"]=>
  int(8)
}
