--TEST--
fatal-error-runs-no-destructor
--FILE--
<?php
class D { function __destruct() { echo "destructor\n"; } }
$d = new D;
trigger_error("boom", E_USER_ERROR);
--EXPECTF--

Deprecated: Passing E_USER_ERROR to trigger_error() is deprecated since 8.4, throw an exception or call exit with a string message instead in %s on line 4

Fatal error: boom in %s on line 4
