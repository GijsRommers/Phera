--TEST--
Array copy-on-write preserves only live external reference holders
--FILE--
<?php
function writeCopy(array $source, int $value) {
    $copy = $source;
    $copy[0] = $value;
    return [$source[0], $copy[0]];
}
function fromForeach() {
    $values = [1, 2, 3];
    foreach ($values as &$v) {}
    unset($v);
    $copy = $values;
    $copy[0] = 10;
    $copy[2] = 30;
    return [$values, $copy];
}
var_dump(fromForeach());

$outer = [[1]];
$alias =& $outer[0][0];
unset($alias);
$copy = $outer;
$copy[0][0] = 2;
var_dump($outer[0][0], $copy[0][0]);

$property = (object) [];
$array = [1];
$property->alias =& $array[0];
unset($property->alias);
var_dump(writeCopy($array, 2));

function viaStatic() {
    static $holder;
    $array = [3];
    $holder =& $array[0];
    unset($holder);
    return $array;
}
var_dump(writeCopy(viaStatic(), 4));

class StaticHolder { public static mixed $alias; }
$array = [3];
$replacement = 0;
StaticHolder::$alias =& $array[0];
var_dump(writeCopy($array, 4), StaticHolder::$alias);
StaticHolder::$alias =& $replacement;
var_dump(writeCopy($array, 5));

$array = [5];
$GLOBALS['holder'] =& $array[0];
unset($GLOBALS['holder']);
var_dump(writeCopy($array, 6));

$array = [7];
$alias =& $array[0];
$closure = static function () use (&$alias) { return $alias; };
unset($alias);
var_dump(writeCopy($array, 8), $closure());
unset($closure);
var_dump(writeCopy($array, 9));

$array = [10];
$other = [];
$other[0] =& $array[0];
var_dump(writeCopy($array, 11), $other[0]);
unset($other[0]);
var_dump(writeCopy($array, 12));

$array = [13];
array_walk($array, static function (&$value) { $value++; });
var_dump(writeCopy($array, 15));

$array = [16];
$alias =& $array[0];
unset($alias);
$copy = $array;
$alias =& $copy[0];
$alias++;
var_dump($array, $copy);
var_dump(serialize($array), serialize($copy));
--EXPECT--
array(2) {
  [0]=>
  array(3) {
    [0]=>
    int(1)
    [1]=>
    int(2)
    [2]=>
    int(3)
  }
  [1]=>
  array(3) {
    [0]=>
    int(10)
    [1]=>
    int(2)
    [2]=>
    int(30)
  }
}
int(1)
int(2)
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(4)
}
array(2) {
  [0]=>
  int(4)
  [1]=>
  int(4)
}
int(4)
array(2) {
  [0]=>
  int(4)
  [1]=>
  int(5)
}
array(2) {
  [0]=>
  int(5)
  [1]=>
  int(6)
}
array(2) {
  [0]=>
  int(8)
  [1]=>
  int(8)
}
int(8)
array(2) {
  [0]=>
  int(8)
  [1]=>
  int(9)
}
array(2) {
  [0]=>
  int(11)
  [1]=>
  int(11)
}
int(11)
array(2) {
  [0]=>
  int(11)
  [1]=>
  int(12)
}
array(2) {
  [0]=>
  int(14)
  [1]=>
  int(15)
}
array(1) {
  [0]=>
  int(16)
}
array(1) {
  [0]=>
  &int(17)
}
string(15) "a:1:{i:0;i:16;}"
string(15) "a:1:{i:0;i:17;}"
