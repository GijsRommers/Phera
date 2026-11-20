--TEST--
cannot-take-arguments
--FILE--
<?php
class X { function __destruct($a) {} }
--EXPECTF--

Fatal error: Method X::__destruct() cannot take arguments in %s on line 2
