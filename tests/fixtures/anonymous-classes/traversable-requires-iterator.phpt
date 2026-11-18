--TEST--
An anonymous class cannot implement Traversable on its own
--FILE--
<?php
new class implements Traversable {};
--EXPECT--
Fatal error: Class Traversable@anonymous must implement interface Traversable as part of either Iterator or IteratorAggregate in Unknown on line 0
