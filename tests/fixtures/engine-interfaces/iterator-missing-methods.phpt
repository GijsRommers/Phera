--TEST--
iterator missing methods
--FILE--
<?php
class IncompleteItems implements Iterator {}
--EXPECTF--
Fatal error: Class IncompleteItems contains 5 abstract methods and must therefore be declared abstract or implement the remaining methods (Iterator::current, Iterator::next, Iterator::key, ...) in %s on line 2
