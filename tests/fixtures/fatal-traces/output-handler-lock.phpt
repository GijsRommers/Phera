--TEST--
Output handler lock
--INI--
fatal_error_backtraces=1
--FILE--
<?php
ob_start(function () { ob_start(); });
echo 'buffer';
ob_end_flush();
--EXPECTF--
Fatal error: ob_start(): Cannot use output buffering in output buffering display handlers in %soutput-handler-lock.php on line 2
Stack trace:
#0 %soutput-handler-lock.php(2): ob_start()
#1 [internal function]: {closure:%soutput-handler-lock.php:2}('buffer', 9)
#2 %soutput-handler-lock.php(4): ob_end_flush()
#3 {main}
