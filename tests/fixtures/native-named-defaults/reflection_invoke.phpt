--TEST--
Native named defaults: reflection invoke
--FILE--
<?php
$r = new ReflectionFunction('json_decode');
var_dump($r->invokeArgs(['json' => '[1]', 'flags' => JSON_THROW_ON_ERROR]));
?>
--EXPECT--
array(1) {
  [0]=>
  int(1)
}
