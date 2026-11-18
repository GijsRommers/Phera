--TEST--
Alias listed as lowercase
--FILE--
<?php
class Original {} class_alias("Original", "Another"); var_dump(array_slice(get_declared_classes(), -2));
--EXPECT--
array(2) {
  [0]=>
  string(8) "Original"
  [1]=>
  string(7) "another"
}
