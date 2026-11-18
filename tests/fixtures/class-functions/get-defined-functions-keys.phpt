--TEST--
Get defined functions keys
--FILE--
<?php
var_dump(array_keys(get_defined_functions()));
--EXPECT--
array(2) {
  [0]=>
  string(8) "internal"
  [1]=>
  string(4) "user"
}
