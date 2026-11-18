--TEST--
arrayaccess missing methods
--FILE--
<?php
class IncompleteStore implements ArrayAccess {}
--EXPECTF--
Fatal error: Class IncompleteStore contains 4 abstract methods and must therefore be declared abstract or implement the remaining methods (ArrayAccess::offsetExists, ArrayAccess::offsetGet, ArrayAccess::offsetSet, ...) in %s on line 2
