--TEST--
constant name callable error
--FILE--
<?php
const N='strlen';const C=(N)(...);
--EXPECTF--

Fatal error: Cannot use dynamic function name in constant expression in %s on line 2
