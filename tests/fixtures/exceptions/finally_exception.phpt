--TEST--
Exceptions: finally_exception
--FILE--
<?php
try {try {throw new Exception("x");} finally {echo "finally";}} catch(Throwable $error){echo $error->getMessage();}
--EXPECTF--
finallyx