--TEST--
Exceptions: handler_reference_warning
--FILE--
<?php
function handler($errno,$message){throw new ErrorException($message);}function &run(){try{return 1;}catch(Throwable $error){echo $error->getMessage();$value=2;return $value;}}set_error_handler("handler");echo run();
--EXPECTF--

Fatal error: Uncaught ErrorException: Only variable references should be returned by reference in %s:2
Stack trace:
#0 %s(2): handler(8, 'Only variable r...', '%s...', 2)
#1 %s(2): run()
#2 {main}
  thrown in %s on line 2
