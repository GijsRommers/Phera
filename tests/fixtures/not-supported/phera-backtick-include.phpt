--TEST--
Backtick deprecation reports the included file and line
--FILE--
<?php
include __DIR__ . '/not-supported-backtick.inc';
echo "after command\n";
--EXPECTF--
Deprecated: The backtick (`) operator is deprecated, use shell_exec() instead in %s/not-supported-backtick.inc on line 2
test
after command
