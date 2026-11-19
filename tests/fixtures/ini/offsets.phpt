--TEST--
INI appends list entries and supports string keys
--FILE--
<?php
var_dump(parse_ini_string("key=scalar\nkey[]=one\nkey[label]=two\nkey[]=three"));
--EXPECT--
array(1) {
  ["key"]=>
  array(3) {
    [0]=>
    string(3) "one"
    ["label"]=>
    string(3) "two"
    [1]=>
    string(5) "three"
  }
}
