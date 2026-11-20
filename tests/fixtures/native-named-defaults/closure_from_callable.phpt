--TEST--
Native named defaults: closure from callable
--FILE--
<?php
$decode = Closure::fromCallable('json_decode');
var_dump($decode('[1]', flags: JSON_THROW_ON_ERROR));
var_dump($decode->__invoke('[1]', flags: JSON_THROW_ON_ERROR));
?>
--EXPECT--
array(1) {
  [0]=>
  int(1)
}
array(1) {
  [0]=>
  int(1)
}
