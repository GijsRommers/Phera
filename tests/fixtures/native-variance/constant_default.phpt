--TEST--
Built-in method compatibility displays its constant default
--FILE--
<?php
class D extends ArrayObject { public function asort(string $flags = ''): true {} }

--EXPECTF--

Fatal error: Declaration of D::asort(string $flags = ''): true must be compatible with ArrayObject::asort(int $flags = SORT_REGULAR): true in %s on line 2
