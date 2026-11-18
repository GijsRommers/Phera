--TEST--
Eval publishes cast and interpolation notices at the evaluated source line
--FILE--
<?php
$name = 'value';
eval('echo (integer) 7, "\n";' . "\n" . 'echo "${name}\n";');
--EXPECTF--
Deprecated: Non-canonical cast (integer) is deprecated, use the (int) cast instead in /private%s(3) : eval()'d code on line 1

Deprecated: Using ${var} in strings is deprecated, use {$var} instead in /private%s(3) : eval()'d code on line 2
7
value
