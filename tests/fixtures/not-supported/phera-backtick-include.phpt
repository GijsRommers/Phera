--TEST--
Backticks report the included file and line
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
$file = __DIR__ . '/not-supported-backtick.inc';
file_put_contents($file, '<?php' . "\n" . 'echo `echo test`;');
include $file;
echo "not reached\n";
--EXPECTF--
Fatal error: Uncaught Error: Call to undefined function file_put_contents() in /private%s:4
Stack trace:
#0 {main}
  thrown in /private%s on line 4
