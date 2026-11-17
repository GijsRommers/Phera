--TEST--
Native classes: exception-dump
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
 var_dump($e);

--EXPECTF--
object(DriverException)#%d (8) {
  ["message":protected]=>
  string(1) "m"
  ["string":"Exception":private]=>
  string(0) ""
  ["code":protected]=>
  string(5) "42P01"
  ["file":protected]=>
  string(%d) "%s"
  ["line":protected]=>
  int(8)
  ["trace":"Exception":private]=>
  array(0) {
  }
  ["previous":"Exception":private]=>
  NULL
  ["errorInfo"]=>
  array(3) {
    [0]=>
    string(5) "42P01"
    [1]=>
    int(7)
    [2]=>
    string(7) "missing"
  }
}
