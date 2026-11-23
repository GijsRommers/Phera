--TEST--
tostring in array string needle
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(in_array("hello", [new Text]));
--EXPECT--
bool(true)
