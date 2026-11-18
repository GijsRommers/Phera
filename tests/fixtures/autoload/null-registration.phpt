--TEST--
Null registration
--FILE--
<?php
var_dump(spl_autoload_register(null), spl_autoload_functions());
--EXPECT--
bool(true)
array(1) {
  [0]=>
  string(12) "spl_autoload"
}
