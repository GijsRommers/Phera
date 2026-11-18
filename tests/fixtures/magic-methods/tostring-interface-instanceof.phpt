--TEST--
tostring interface instanceof
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(new Text instanceof Stringable);
--EXPECT--
bool(true)
