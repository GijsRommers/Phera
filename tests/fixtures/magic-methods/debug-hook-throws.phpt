--TEST--
An exception from __debugInfo causes a warning and a fatal error
--FILE--
<?php
class Report {
    function __debugInfo() { throw new Exception("debug failure"); }
}
try { var_dump(new Report); } catch (Exception $e) { echo "caught"; }
--EXPECTF--
Warning: Uncaught Exception: debug failure in %s:3
Stack trace:
#0 [internal function]: Report->__debugInfo()
#1 %s(5): var_dump(Object(Report))
#2 {main}
  thrown in %s on line 3

Fatal error: __debuginfo() must return an array in %s on line 5
