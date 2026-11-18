--TEST--
tostring native multiple parameters
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(strpos(new Text, new Text));
--EXPECT--
int(0)
