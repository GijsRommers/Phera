--TEST--
Class conformance: foreach-object-reference-typed-error
--FILE--
<?php class Box {public int $first=1;} $box=new Box;foreach($box as &$value){$value=[];}
--EXPECTF--

Fatal error: Uncaught TypeError: Cannot assign array to reference held by property Box::$first of type int in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
