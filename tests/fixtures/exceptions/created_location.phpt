--TEST--
Exceptions: created_location
--FILE--
<?php
$error = new Exception("message", 17); echo $error->getMessage(), "|", $error->getCode(), "|", $error->getLine(), "|", basename($error->getFile()), "\n", $error->getTraceAsString(), "\n";
--EXPECTF--
message|17|2|%s
#0 {main}
