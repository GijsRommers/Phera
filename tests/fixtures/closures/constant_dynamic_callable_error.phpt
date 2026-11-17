--TEST--
constant dynamic callable error
--FILE--
<?php
const C=$f(...);
--EXPECTF--

Fatal error: Cannot use dynamic function name in constant expression in %s on line 2
