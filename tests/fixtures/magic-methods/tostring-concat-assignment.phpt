--TEST--
tostring concat assignment
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$s = "1"; $s .= new Text; echo $s;
--EXPECT--
1hello
