--TEST--
Native named defaults: unpack
--FILE--
<?php
var_dump(json_decode(...['json' => '[1]', 'flags' => JSON_THROW_ON_ERROR]));
?>
--EXPECT--
array(1) {
  [0]=>
  int(1)
}
