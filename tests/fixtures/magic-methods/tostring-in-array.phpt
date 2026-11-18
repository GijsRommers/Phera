--TEST--
tostring in array
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(in_array(new Text, ["a", "hello"]));
--EXPECT--
bool(true)
