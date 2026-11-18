--TEST--
tostring result weak scalar
--FILE--
<?php
class Text { function __toString() { return 42; } }
var_dump((string) new Text);
--EXPECT--
string(2) "42"
