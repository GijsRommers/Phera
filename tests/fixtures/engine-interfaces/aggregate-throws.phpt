--TEST--
aggregate throws
--FILE--
<?php
class Broken implements IteratorAggregate {function getIterator(): Traversable { throw new Exception("hook"); }}
try { foreach (new Broken as $v) {} } catch(Exception $e) {echo $e->getMessage(),"\n";}
--EXPECT--
hook
