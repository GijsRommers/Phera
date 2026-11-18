--TEST--
enums: enum case in exception trace
--FILE--
<?php
function fail($argument) { throw new Exception(); }
enum E {case A;}
fail(E::A);
--EXPECTF--
Fatal error: Uncaught Exception in %s:2
Stack trace:
#0 %s(4): fail(E::A)
#1 {main}
  thrown in %s on line 2
