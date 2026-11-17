--TEST--
Exceptions: fatal_redeclare_class
--FILE--
<?php
try{class stdClass{}}catch(Throwable){echo "bad";}
--EXPECTF--

Fatal error: Cannot redeclare class stdClass in %s on line 2
