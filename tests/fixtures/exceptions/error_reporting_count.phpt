--TEST--
Exceptions: error_reporting_count
--FILE--
<?php
try { error_reporting(1, 2); } catch (Throwable $error) { echo get_class($error), "|", $error->getMessage(), "\n", $error->getTraceAsString(), "\n"; }
--EXPECTF--
ArgumentCountError|error_reporting() expects at most 1 argument, 2 given
#0 %s(2): error_reporting(1, 2)
#1 {main}
