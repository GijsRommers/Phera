--TEST--
preg_grep encounters invalid UTF-8 after a match
--FILE--
<?php
var_dump(preg_grep('/a/u', ['first' => 'a', 'invalid' => "\xff", 'last' => 'a']));
echo preg_last_error_msg(), PHP_EOL;
--EXPECT--
array(1) {
  ["first"]=>
  string(1) "a"
}
Malformed UTF-8 characters, possibly incorrectly encoded
