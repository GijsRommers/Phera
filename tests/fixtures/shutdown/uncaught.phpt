--TEST--
Shutdown runs after an uncaught throwable
--FILE--
<?php
register_shutdown_function(fn() => print "shutdown\n");
throw new RuntimeException('uncaught');
--EXPECTF--

Fatal error: Uncaught RuntimeException: uncaught in %s:3
Stack trace:
#0 {main}
  thrown in %s on line 3
shutdown
