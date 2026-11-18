--TEST--
tostring concat assignment object
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$s = new Text; $s .= "!"; var_dump($s);
--EXPECT--
string(6) "hello!"
