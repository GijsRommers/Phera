--TEST--
An Iterator enum must implement all methods of the protocol
--FILE--
<?php
enum Cases implements Iterator { case One; }
--EXPECTF--
Fatal error: Enum Cases must implement 5 abstract methods (Iterator::current, Iterator::next, Iterator::key, ...) in %s on line 2
