--TEST--
Get defined functions user order
--FILE--
<?php
function FirstFn() {} function SecondFn() {} var_dump(get_defined_functions()["user"]);
--EXPECT--
array(2) {
  [0]=>
  string(7) "firstfn"
  [1]=>
  string(8) "secondfn"
}
