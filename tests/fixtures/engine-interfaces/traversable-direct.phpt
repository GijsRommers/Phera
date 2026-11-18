--TEST--
traversable direct
--FILE--
<?php

class Wrong implements Traversable {}
--EXPECT--
Fatal error: Class Wrong must implement interface Traversable as part of either Iterator or IteratorAggregate in Unknown on line 0
