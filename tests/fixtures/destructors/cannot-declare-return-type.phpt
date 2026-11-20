--TEST--
cannot-declare-return-type
--FILE--
<?php
class X { function __destruct(): void {} }
--EXPECTF--

Fatal error: Method X::__destruct() cannot declare a return type in %s on line 2
