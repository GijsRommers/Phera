--TEST--
Foreach by reference links the loop variable to the generator's local
--FILE--
<?php
function &numbers(&$values) {
    foreach ($values as &$value) {
        yield $value;
    }
}
$values = [1, 3, 5];
foreach (numbers($values) as &$value) {
    $value *= 2;
}
unset($value);
var_dump($values);
try {
    foreach ((fn() => yield 1)() as &$value) {}
} catch (Exception $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(6)
  [2]=>
  int(10)
}
You can only iterate a generator by-reference if it declared that it yields by-reference
