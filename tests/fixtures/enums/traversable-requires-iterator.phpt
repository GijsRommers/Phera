--TEST--
An enum cannot implement Traversable on its own
--FILE--
<?php
enum Cases implements Traversable { case One; }
--EXPECT--
Fatal error: Enum Cases must implement interface Traversable as part of either Iterator or IteratorAggregate in Unknown on line 0
