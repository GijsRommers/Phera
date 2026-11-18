--TEST--
tostring interface parameter
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
function accept(Stringable $v) { echo $v; } accept(new Text);
--EXPECT--
hello
