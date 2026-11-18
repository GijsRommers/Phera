--TEST--
tostring key existence
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
try { var_dump(array_key_exists(new Text, [])); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
Cannot access offset of type Text on array
