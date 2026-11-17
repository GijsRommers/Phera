--TEST--
Native classes: exception-uncaught
--FILE--
<?php
class DriverException extends PDOException {
 public function setCode($code) {
 $this->code=$code;
 }
 }

$e=new DriverException("m");
 $e->setCode("42P01");
 $e->errorInfo=["42P01",7,"missing"];
 throw $e;

--EXPECTF--

Fatal error: Uncaught DriverException: m in %s:8
Stack trace:
#0 {main}
  thrown in %s on line 8
