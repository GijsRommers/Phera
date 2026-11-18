--TEST--
tostring concat left
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
echo new Text . "!";
--EXPECT--
hello!
