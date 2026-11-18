--TEST--
tostring strict interface
--FILE--
<?php
declare(strict_types=1); class Text { function __toString() { return "hello"; } }
function accept(Stringable $v) { echo $v; } accept(new Text);
--EXPECT--
hello
