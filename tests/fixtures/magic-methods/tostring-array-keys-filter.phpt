--TEST--
tostring array keys filter
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(array_keys(["a" => new Text], "hello"));
--EXPECT--
array(1) {
  [0]=>
  string(1) "a"
}
