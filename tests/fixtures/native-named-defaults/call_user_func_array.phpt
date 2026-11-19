--TEST--
Native named defaults: call user func array
--FILE--
<?php
var_dump(call_user_func_array('json_decode', ['json' => '[1]', 'flags' => JSON_THROW_ON_ERROR]));
?>
--EXPECT--
array(1) {
  [0]=>
  int(1)
}
