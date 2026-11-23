--TEST--
An enum cannot implement both Iterator protocols
--FILE--
<?php
enum Cases implements Iterator, IteratorAggregate { case One; }
--EXPECTF--
Fatal error: Class Cases cannot implement both Iterator and IteratorAggregate at the same time in %s on line 2
