--TEST--
tostring echo
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
echo new Text, " world";
--EXPECT--
hello world
