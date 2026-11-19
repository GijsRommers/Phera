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
bool(true)
array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH
array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH
