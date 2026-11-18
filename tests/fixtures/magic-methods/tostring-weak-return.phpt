--TEST--
tostring weak return
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
function result(): string { return new Text; } var_dump(result());
--EXPECT--
string(5) "hello"
