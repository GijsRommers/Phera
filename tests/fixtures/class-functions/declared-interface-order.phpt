--TEST--
Declared interface order
--FILE--
<?php
interface Another {} interface Example {} var_dump(array_slice(get_declared_interfaces(),-2));
--EXPECT--
array(2) {
  [0]=>
  string(7) "Another"
  [1]=>
  string(7) "Example"
}
