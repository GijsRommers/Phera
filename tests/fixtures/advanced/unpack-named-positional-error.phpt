--TEST--
unpack-named-positional-error
--FILE--
<?php function collect(...$rest) {} collect(...['name' => 1, 0 => 2]);

--EXPECTF--

Fatal error: Uncaught Error: Cannot use positional argument after named argument during unpacking in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
