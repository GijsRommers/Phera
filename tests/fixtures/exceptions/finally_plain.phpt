--TEST--
Exceptions: finally_plain
--FILE--
<?php
try {echo "try";} finally {echo "finally";} echo "after";
--EXPECTF--
tryfinallyafter