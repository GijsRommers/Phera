--TEST--
Register case insensitive duplicate
--FILE--
<?php
function Actual($class) {} spl_autoload_register("actual"); spl_autoload_register("ACTUAL", prepend:true); var_dump(spl_autoload_functions());
--EXPECT--
array(1) {
  [0]=>
  string(6) "Actual"
}
