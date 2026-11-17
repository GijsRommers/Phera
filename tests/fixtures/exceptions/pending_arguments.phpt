--TEST--
Exceptions: pending_arguments
--FILE--
<?php
function sink($first,$second){echo "bad";} function fail(){throw new Exception("x");} try {sink(new stdClass,fail());} catch(Throwable $error){echo $error->getMessage();} echo "ok";
--EXPECTF--
xok