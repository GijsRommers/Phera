--TEST--
enums: enum case in exception trace
--FILE--
<?php
function fail($argument) { throw new Exception(); }
enum E {case A;}
fail(E::A);
--EXPECTF--
Fatal error: Uncaught Exception in /private%s:2
Stack trace:
#0 /private%s(4): fail(E::A)
#1 {main}
  thrown in /private%s on line 2
