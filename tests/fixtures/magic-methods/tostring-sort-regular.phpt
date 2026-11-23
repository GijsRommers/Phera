--TEST--
tostring sort regular
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$items = ["z", new Text, "a"]; sort($items); foreach ($items as $item) { echo $item; }
--EXPECT--
ahelloz
