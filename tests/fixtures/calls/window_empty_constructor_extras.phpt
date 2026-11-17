--TEST--
Call window: empty constructor extras
--FILE--
<?php
class A{}$a=new A(1,[2],z:3);var_dump($a instanceof A);
--EXPECTF--

Fatal error: Uncaught Error: Unknown named parameter $z in %s:2
Stack trace:
#0 {main}
  thrown in %s on line 2
