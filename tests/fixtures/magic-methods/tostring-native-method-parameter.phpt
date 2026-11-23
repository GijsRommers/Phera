--TEST--
tostring native method parameter
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
echo (new Exception(new Text))->getMessage();
--EXPECT--
hello
