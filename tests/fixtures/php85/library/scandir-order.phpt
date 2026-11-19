--TEST--
scandir sorting order validation
--FILE--
<?php
mkdir('fixture-scandir-order');
try { var_dump(scandir('fixture-scandir-order', 42)); }
catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
rmdir('fixture-scandir-order');
--EXPECT--
array(2) {
  [0]=>
  string(2) ".."
  [1]=>
  string(1) "."
}
