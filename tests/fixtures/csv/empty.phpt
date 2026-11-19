--TEST--
An empty CSV string has one null field
--FILE--
<?php
var_dump(str_getcsv('', escape: ''));
--EXPECT--
array(1) {
  [0]=>
  NULL
}
