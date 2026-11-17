--TEST--
Class conformance: type-error-property-object
--FILE--
<?php class P {} class Box {public P $value;} $box=new Box;$box->value=new stdClass;
--EXPECTF--

Fatal error: Uncaught TypeError: Cannot assign stdClass to property Box::$value of type P in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
