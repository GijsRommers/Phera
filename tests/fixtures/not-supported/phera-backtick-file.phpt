--TEST--
Backticks deprecate before top-level code runs
--FILE--
<?php
echo "after command\n";
echo `echo test`;
--EXPECTF--
Deprecated: The backtick (`) operator is deprecated, use shell_exec() instead in %s on line 3
after command
test
