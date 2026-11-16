--TEST--
Class conformance: reference-typed-static-error
--FILE--
<?php class Box {public static int $value=1;} $alias=&Box::$value;$alias=[];
--EXPECTF--

Fatal error: Uncaught TypeError: Cannot assign array to reference held by property Box::$value of type int in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
