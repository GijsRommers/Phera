--TEST--
tostring sort natural
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$items = ["z", new Text, "a"]; natsort($items); foreach ($items as $item) { echo $item; }
--EXPECT--
ahelloz
