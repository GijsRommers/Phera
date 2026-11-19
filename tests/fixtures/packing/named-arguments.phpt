--TEST--
Pack checks its format before rejecting extra named arguments
--FILE--
<?php
foreach ([fn() => pack(format: 'C', extra: 1), fn() => pack(format: 'C', values: 1), fn() => pack(extra: 1), fn() => pack(format: [], extra: 1)] as $call) {
    try {echo bin2hex($call()), "\n";}
    catch (Throwable $error) {echo get_class($error), ' ', $error->getMessage(), "\n";}
}
--EXPECT--
ArgumentCountError pack() does not accept unknown named parameters
ArgumentCountError pack() does not accept unknown named parameters
ArgumentCountError pack() expects at least 1 argument, 0 given
TypeError pack(): Argument #1 ($format) must be of type string, array given
