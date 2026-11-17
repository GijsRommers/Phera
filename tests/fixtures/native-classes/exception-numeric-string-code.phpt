--TEST--
Native classes: exception-numeric-string-code
--FILE--
<?php
class DriverException extends PDOException {
 public function setCode($code) {
 $this->code=$code;
 }
 }

$e=new DriverException("m");
 $e->setCode("00000");
 var_dump($e->getCode());
 echo $e,"\n";

--EXPECTF--
string(5) "00000"
DriverException: m in %s:8
Stack trace:
#0 {main}
