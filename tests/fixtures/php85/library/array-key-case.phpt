--TEST--
array_change_key_case mode validation
--FILE--
<?php
foreach ([CASE_LOWER, CASE_UPPER, 42] as $case) {
    try { var_dump(array_change_key_case(['MiXeD' => 5], $case)); }
    catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
}
--EXPECT--
array(1) {
  ["mixed"]=>
  int(5)
}
array(1) {
  ["MIXED"]=>
  int(5)
}
array(1) {
  ["MIXED"]=>
  int(5)
}
