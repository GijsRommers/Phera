--TEST--
warning_variable
--FILE--
<?php
echo $missing;
--EXPECTF--

Warning: Undefined variable $missing in %s on line 2
