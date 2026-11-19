--TEST--
Array snapshots use copy on write and preserve object identity
--FILE--
<?php
function inspect($array, $object) {
    $args = debug_backtrace()[0]['args'];
    $array[] = 11;
    $object->value = 7;
    var_dump($args[0], $args[1] === $object, $args[1]->value);
}
inspect([3], new stdClass());
--EXPECT--
array(1) {
  [0]=>
  int(3)
}
bool(true)
int(7)
