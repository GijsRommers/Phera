--TEST--
Exceptions: finally_chain
--FILE--
<?php
try {try {throw new Error("x");} finally {throw new Exception("y");}} catch(Throwable $error) {echo $error->getMessage(),"|",$error->getPrevious()->getMessage();}
--EXPECTF--
y|x