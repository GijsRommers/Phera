--TEST--
__toString applies the declaring file strictness
--FILE--
<?php
declare(strict_types=1);
class Text { function __toString() { return 42; } }
try { echo new Text; } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
Text::__toString(): Return value must be of type string, int returned
