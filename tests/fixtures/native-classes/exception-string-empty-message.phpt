--TEST--
Native classes: exception-string-empty-message
--FILE--
<?php
class DriverException extends PDOException {
 public function setCode($code) {
 $this->code=$code;
 }
 }

$e=new DriverException;
 $e->setCode("HY000");
 echo (string)$e,"\n";

--EXPECTF--
DriverException in %s:8
Stack trace:
#0 {main}
