--TEST--
__toString falling through has a missing return value
--FILE--
<?php
class Text { function __toString() {} }
try { echo new Text; } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
Text::__toString(): Return value must be of type string, none returned
