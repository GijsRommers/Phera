--TEST--
tostring settype
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$text = new Text; var_dump(settype($text, "string"), $text);
--EXPECT--
bool(true)
string(5) "hello"
