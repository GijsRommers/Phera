--TEST--
tostring implode separator
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
echo implode(new Text, ["a", "b"]);
--EXPECT--
ahellob
