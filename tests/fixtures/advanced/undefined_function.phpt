--TEST--
undefined_function
--FILE--
<?php
absent();
--EXPECTF--

Fatal error: Uncaught Error: Call to undefined function absent() in %s:2
Stack trace:
#0 {main}
  thrown in %s on line 2
