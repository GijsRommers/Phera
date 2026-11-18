--TEST--
tostring array search
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
var_dump(array_search("hello", ["a" => new Text]));
--EXPECT--
string(1) "a"
