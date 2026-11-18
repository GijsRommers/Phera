--TEST--
tostring result direct method
--FILE--
<?php
class Text { function __toString() { return 42; } }
var_dump((new Text)->__toString());
--EXPECT--
string(2) "42"
