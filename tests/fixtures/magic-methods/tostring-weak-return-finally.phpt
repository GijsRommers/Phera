--TEST--
tostring weak return finally
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
function result(): string { try { return new Text; } finally { echo "finally\n"; } } var_dump(result());
--EXPECT--
finally
string(5) "hello"
