--TEST--
Exceptions: exception_handler_throw
--FILE--
<?php
function handler($error){throw new Error("handler");}set_exception_handler("handler");throw new Exception("original");
--EXPECTF--

Fatal error: Uncaught Error: handler in %s:2
Stack trace:
#0 [internal function]: handler(Object(Exception))
#1 {main}
  thrown in %s on line 2
