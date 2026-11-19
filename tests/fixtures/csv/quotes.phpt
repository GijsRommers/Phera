--TEST--
CSV supports doubled enclosures with empty escape
--FILE--
<?php
var_dump(str_getcsv('"a""b","c,d",e', escape: ''));
--EXPECT--
array(3) {
  [0]=>
  string(3) "a"b"
  [1]=>
  string(3) "c,d"
  [2]=>
  string(1) "e"
}
