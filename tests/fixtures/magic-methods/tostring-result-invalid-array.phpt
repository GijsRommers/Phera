--TEST--
__toString has an implicit string return type
--FILE--
<?php
class Text { function __toString() { return []; } }
try { echo new Text; } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
Text::__toString(): Return value must be of type string, array returned
