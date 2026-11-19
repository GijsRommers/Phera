--TEST--
array_filter mode validation
--FILE--
<?php
var_dump(defined('ARRAY_FILTER_USE_VALUE'));
foreach ([[0, 1], []] as $values) {
    try { var_dump(array_filter($values, null, -1)); }
    catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
}
--EXPECT--
bool(false)
array(1) {
  [1]=>
  int(1)
}
array(0) {
}
