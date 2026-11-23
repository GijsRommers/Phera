--TEST--
tostring native parameter
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(strlen(new Text));
--EXPECT--
int(5)
