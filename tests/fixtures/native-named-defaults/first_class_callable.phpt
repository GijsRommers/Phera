--TEST--
Native named defaults: first class callable
--FILE--
<?php
$decode = json_decode(...);
var_dump($decode('[1]', flags: JSON_THROW_ON_ERROR));
?>
--EXPECT--
array(1) {
  [0]=>
  int(1)
}
