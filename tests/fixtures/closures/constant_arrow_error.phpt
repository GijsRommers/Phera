--TEST--
constant arrow error
--FILE--
<?php
const C=static fn()=>1;
--EXPECTF--

Fatal error: Constant expression contains invalid operations in %s on line 2
