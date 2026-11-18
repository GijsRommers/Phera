--TEST--
tostring equality
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(new Text == "hello", "hello" == new Text);
--EXPECT--
bool(true)
bool(true)
