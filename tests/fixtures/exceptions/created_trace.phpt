--TEST--
Exceptions: created_trace
--FILE--
<?php
function factory($number) { return new Exception("message"); } $error = factory(17); echo $error->getMessage(), "|", $error->getCode(), "|", $error->getLine(), "\n", $error->getTraceAsString(), "\n";
--EXPECTF--
message|0|2
#0 %s(2): factory(17)
#1 {main}
