--TEST--
include-redeclare5
--FILE--
<?php include __DIR__.'/function5.inc'; include __DIR__.'/function5.inc';
--EXPECTF--

Fatal error: Cannot redeclare function included5() (previously declared in %s:1) in %s on line 1
