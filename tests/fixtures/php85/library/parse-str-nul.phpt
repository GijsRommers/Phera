--TEST--
A NUL byte in parse_str and the by-reference result
--FILE--
<?php
$result = 'previous';
try { parse_str("first=1\0&second=2", $result); }
catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
var_dump($result);
--EXPECT--
array(1) {
  ["first"]=>
  string(1) "1"
}
