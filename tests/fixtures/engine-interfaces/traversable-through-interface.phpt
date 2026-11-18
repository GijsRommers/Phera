--TEST--
traversable through interface
--FILE--
<?php

interface Contract extends Traversable {} class Wrong implements Contract {}
--EXPECT--
Fatal error: Class Wrong must implement interface Traversable as part of either Iterator or IteratorAggregate in Unknown on line 0
