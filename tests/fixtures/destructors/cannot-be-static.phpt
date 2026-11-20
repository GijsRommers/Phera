--TEST--
cannot-be-static
--FILE--
<?php
class X { static function __destruct() {} }
--EXPECTF--

Fatal error: Method X::__destruct() cannot be static in %s on line 2
