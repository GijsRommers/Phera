--TEST--
Exceptions: storage_array_failure
--FILE--
<?php
$values=["abc"];try{$values[0][]="d";}catch(Throwable){}var_dump($values);
--EXPECTF--
array(1) {
  [0]=>
  string(3) "abc"
}
