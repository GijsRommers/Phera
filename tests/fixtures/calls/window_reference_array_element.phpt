--TEST--
Call window: reference array element
--FILE--
<?php
function f(&$x){$x++;}$a=['k'=>3];f($a['k']);var_dump($a);
--EXPECT--
array(1) {
  ["k"]=>
  int(4)
}
