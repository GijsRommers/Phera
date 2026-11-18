--TEST--
aggregate wrong return
--FILE--
<?php
class Broken implements IteratorAggregate {function getIterator(): Traversable { return []; }}
try { foreach (new Broken as $v) {} } catch(TypeError $e) {echo $e->getMessage(),"\n";}
--EXPECT--
Broken::getIterator(): Return value must be of type Traversable, array returned
