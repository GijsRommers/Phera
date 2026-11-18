--TEST--
Declared class order
--FILE--
<?php
class Another {} class Example {} var_dump(array_slice(get_declared_classes(),-2));
--EXPECT--
array(2) {
  [0]=>
  string(7) "Another"
  [1]=>
  string(7) "Example"
}
