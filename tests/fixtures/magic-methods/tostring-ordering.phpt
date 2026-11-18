--TEST--
tostring ordering
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(new Text < "world", "hello" <=> new Text);
--EXPECT--
bool(true)
int(0)
