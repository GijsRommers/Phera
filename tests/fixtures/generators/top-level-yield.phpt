--TEST--
Yield requires a function body
--FILE--
<?php
yield;
--EXPECTF--

Fatal error: The "yield" expression can only be used inside a function in %s on line 2
