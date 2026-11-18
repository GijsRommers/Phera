--TEST--
tostring weak variadic parameter
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
function accept(string ...$v) { var_dump($v); } accept(new Text, named: new Text);
--EXPECT--
array(2) {
  [0]=>
  string(5) "hello"
  ["named"]=>
  string(5) "hello"
}
