--TEST--
Native classes: exception-error-info
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
 var_dump($e->getCode(),$e->errorInfo);

--EXPECT--
string(5) "42P01"
array(3) {
  [0]=>
  string(5) "42P01"
  [1]=>
  int(7)
  [2]=>
  string(7) "missing"
}
