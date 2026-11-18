--TEST--
A nested var_dump from __debugInfo reports recursion
--FILE--
<?php
class Report {
    function __debugInfo() { var_dump($this); return ['field' => 1]; }
}
var_dump(new Report);
--EXPECT--
*RECURSION*
object(Report)#1 (1) {
  ["field"]=>
  int(1)
}
