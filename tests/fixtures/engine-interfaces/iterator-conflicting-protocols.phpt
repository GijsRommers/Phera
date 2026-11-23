--TEST--
iterator conflicting protocols
--FILE--
<?php
class InvalidItems implements Iterator, IteratorAggregate {}
--EXPECTF--
Fatal error: Class InvalidItems cannot implement both Iterator and IteratorAggregate at the same time in %s on line 2
