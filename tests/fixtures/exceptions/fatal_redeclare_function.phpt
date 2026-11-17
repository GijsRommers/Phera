--TEST--
Exceptions: fatal_redeclare_function
--FILE--
<?php
function duplicate(){}try{function duplicate(){}}catch(Throwable){echo "bad";}
--EXPECTF--

Fatal error: Cannot redeclare function duplicate() (previously declared in %s:2) in %s on line 2
