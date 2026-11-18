--TEST--
Backticks fail to compile before top-level code runs
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
echo "not reached\n";
$result = `echo test`;
--EXPECTF--
Fatal error: The backtick operator is not supported by Phera: deprecated since PHP 8.5, use shell_exec() instead in /private%s on line 4
