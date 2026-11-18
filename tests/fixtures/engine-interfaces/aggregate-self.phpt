--TEST--
aggregate self
--FILE--
<?php
class SelfItems implements IteratorAggregate {function getIterator(): Traversable { return $this; }}
try { foreach (new SelfItems as $v) {} } catch(Exception $e) {echo $e->getMessage(),"\n";}
--EXPECT--
Objects returned by SelfItems::getIterator() must be traversable or implement interface Iterator
