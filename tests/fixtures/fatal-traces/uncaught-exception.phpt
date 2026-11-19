--TEST--
Uncaught exception
--INI--
fatal_error_backtraces=1
--FILE--
<?php
function fail($arg) { throw new RuntimeException('uncaught'); }
fail(12);
--EXPECTF--
Fatal error: Uncaught RuntimeException: uncaught in %suncaught-exception.php:2
Stack trace:
#0 %suncaught-exception.php(3): fail(12)
#1 {main}
  thrown in %suncaught-exception.php on line 2
