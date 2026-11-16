--TEST--
Class members: error-write-null
--FILE--
<?php $a=null; $a->x=1;
--EXPECTF--

Fatal error: Uncaught Error: Attempt to assign property "x" on null in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
