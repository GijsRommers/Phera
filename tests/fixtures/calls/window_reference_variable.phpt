--TEST--
Call window: reference variable
--FILE--
<?php
function f(&$x){$x=['changed'];} $x=1;f($x);var_dump($x);
--EXPECT--
array(1) {
  [0]=>
  string(7) "changed"
}
