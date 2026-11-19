--TEST--
Backtick deprecation reports the evaluated code and line
--FILE--
<?php
eval("\necho `echo test`;");
echo "after command\n";
--EXPECTF--
Deprecated: The backtick (`) operator is deprecated, use shell_exec() instead in %s(2) : eval()'d code on line 2
test
after command
