--TEST--
Exceptions: rethrow_identity
--FILE--
<?php
$original=new Exception("x"); try {try {throw $original;} catch(Throwable $error){throw $error;}} catch(Throwable $error){var_dump($error===$original); echo $error->getLine(),"|",$error->getTraceAsString();}
--EXPECTF--
bool(true)
2|#0 {main}