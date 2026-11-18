--TEST--
tostring weak method parameter
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
class User { function read(string $v) { var_dump($v); } } (new User)->read(new Text);
--EXPECT--
string(5) "hello"
