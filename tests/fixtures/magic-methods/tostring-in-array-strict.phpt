--TEST--
tostring in array strict
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(in_array(new Text, ["hello"], true));
--EXPECT--
bool(false)
