--TEST--
tostring sort string
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$items = ["z", new Text, "a"]; rsort($items, SORT_STRING); foreach ($items as $item) { echo $item; }
--EXPECT--
zhelloa
