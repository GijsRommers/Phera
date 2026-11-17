--TEST--
Exceptions: silence_unwind
--FILE--
<?php
function fail(){throw new Exception("x");} try{@fail();}catch(Throwable){echo "caught";} echo $missing;
--EXPECTF--
caught
Warning: Undefined variable $missing in %s on line 2
