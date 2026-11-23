--TEST--
tostring strval
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(strval(new Text));
--EXPECT--
string(5) "hello"
