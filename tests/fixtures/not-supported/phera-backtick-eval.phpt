--TEST--
Backticks report the evaluated code and line
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
eval("\necho `echo test`;");
echo "not reached\n";
--EXPECTF--
Fatal error: The backtick operator is not supported by Phera: deprecated since PHP 8.5, use shell_exec() instead in %s(3) : eval()'d code on line 2
