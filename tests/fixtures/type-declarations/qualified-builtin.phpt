--TEST--
A builtin declaration type must be unqualified
--FILE--
<?php
function invalid(): \iterable {}
--EXPECTF--
Fatal error: Type declaration 'iterable' must be unqualified in %s on line 2
