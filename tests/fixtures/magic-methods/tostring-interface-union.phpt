--TEST--
tostring interface union
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
function accept(string|Stringable $v) { var_dump(is_object($v)); } accept(new Text);
--EXPECT--
bool(true)
