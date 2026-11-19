--TEST--
CSV missing an escape emits the PHP 8.5 deprecation
--FILE--
<?php
var_dump(str_getcsv('a,b'));
--EXPECTF--
Deprecated: str_getcsv(): the $escape parameter must be provided as its default value will change in %s/default-escape.php on line 2
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
}
