--TEST--
Debug properties can contain the object itself
--FILE--
<?php
class Report { function __debugInfo() { return ["self"=>$this]; } }
var_dump(new Report);
--EXPECT--
object(Report)#1 (1) {
  ["self"]=>
  *RECURSION*
}
