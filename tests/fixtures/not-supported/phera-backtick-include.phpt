--TEST--
Backticks report the included file and line
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
include __DIR__ . '/not-supported-backtick.inc';
echo "not reached\n";
--EXPECTF--
Fatal error: The backtick operator is not supported by Phera: deprecated since PHP 8.5, use shell_exec() instead in %s/not-supported-backtick.inc on line 2
