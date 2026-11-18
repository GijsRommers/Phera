--TEST--
tostring implode elements
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
echo implode("-", [new Text, 1, new Text]);
--EXPECT--
hello-1-hello
