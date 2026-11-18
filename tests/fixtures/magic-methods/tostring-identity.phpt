--TEST--
tostring identity
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(new Text === "hello");
--EXPECT--
bool(false)
