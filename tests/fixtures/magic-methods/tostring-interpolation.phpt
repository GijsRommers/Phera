--TEST--
tostring interpolation
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$t = new Text; echo "a $t b";
--EXPECT--
a hello b
