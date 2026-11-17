--TEST--
Class conformance: type-error-return-string
--FILE--
<?php class P {} function give(): P {return 'bad';} give();
--EXPECTF--

Fatal error: Uncaught TypeError: give(): Return value must be of type P, string returned in %s:1
Stack trace:
#0 %s(1): give()
#1 {main}
  thrown in %s on line 1
