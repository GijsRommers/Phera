--TEST--
Native classes: exception-previous-chain
--FILE--
<?php
class DriverException extends PDOException {
 public function setCode($code) {
 $this->code=$code;
 }
 }

$e=new DriverException("first");
 $e->setCode("42P01");
 $next=new DriverException("second",0,$e);
 $next->setCode("HY000");
 var_dump($next->getCode(),$next->getPrevious()->getCode());
 echo (string)$next,"\n";

--EXPECTF--
string(5) "HY000"
string(5) "42P01"
DriverException: first in %s:8
Stack trace:
#0 {main}

Next DriverException: second in %s:10
Stack trace:
#0 {main}
