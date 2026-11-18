--TEST--
tostring print
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
print new Text;
--EXPECT--
hello
