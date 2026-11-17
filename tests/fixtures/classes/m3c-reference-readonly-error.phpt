--TEST--
Class conformance: reference-readonly-error
--FILE--
<?php class Box {public readonly int $value;function __construct(){$this->value=1;}}$box=new Box;$alias=&$box->value;
--EXPECTF--

Fatal error: Uncaught Error: Cannot indirectly modify readonly property Box::$value in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
