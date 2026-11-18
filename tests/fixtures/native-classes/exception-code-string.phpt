--TEST--
Native classes: exception-code-string
--FILE--
<?php
class DriverException extends PDOException {
 public function setCode($code) {
 $this->code=$code;
 }
 }

$e=new DriverException("m");
 $e->setCode("42P01");
 var_dump($e->getCode());

--EXPECT--
string(5) "42P01"
