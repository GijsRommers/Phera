--TEST--
tostring weak parameter
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
function accept(string $v) { var_dump($v); } accept(new Text);
--EXPECT--
string(5) "hello"
