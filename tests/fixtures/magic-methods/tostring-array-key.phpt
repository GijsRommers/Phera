--TEST--
tostring array key
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
$a = []; try { $a[new Text] = 1; } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
Cannot access offset of type Text on array
