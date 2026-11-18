--TEST--
tostring cast
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump((string) new Text);
--EXPECT--
string(5) "hello"
