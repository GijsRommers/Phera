--TEST--
Exceptions: pending_expression
--FILE--
<?php
function fail(){throw new Exception("x");} try {$value=3+fail();} catch(Throwable $error){echo $error->getMessage();} echo "ok";
--EXPECTF--
xok