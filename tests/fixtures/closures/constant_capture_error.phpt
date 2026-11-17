--TEST--
constant capture error
--FILE--
<?php
$x=1;const C=static function()use($x){};
--EXPECTF--

Fatal error: Cannot use(...) variables in constant expression in %s on line 2
