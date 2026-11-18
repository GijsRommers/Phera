--TEST--
Native classes: exception-string
--FILE--
<?php
class DriverException extends PDOException {
 public function setCode($code) {
 $this->code=$code;
 }
 }

$e=new DriverException("m");
 $e->setCode("42P01");
 echo (string)$e,"\n";

--EXPECTF--
DriverException: m in %s:8
Stack trace:
#0 {main}
