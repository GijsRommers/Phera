--TEST--
Exceptions: handler_return_warning
--FILE--
<?php
function handler($errno,$message){throw new ErrorException($message);}function run(){try{return $missing;}catch(Throwable $error){return $error->getMessage();}}set_error_handler("handler");echo run();
--EXPECTF--

Fatal error: Uncaught ErrorException: Undefined variable $missing in %s:2
Stack trace:
#0 %s(2): handler(2, 'Undefined varia...', '%s...', 2)
#1 %s(2): run()
#2 {main}
  thrown in %s on line 2
