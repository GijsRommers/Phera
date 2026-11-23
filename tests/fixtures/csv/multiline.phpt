--TEST--
CSV keeps line endings inside enclosed fields
--FILE--
<?php
var_dump(str_getcsv("\"a\r\nb\",c\r\n", escape: ''));
--EXPECT--
array(2) {
  [0]=>
  string(4) "a
b"
  [1]=>
  string(1) "c"
}
