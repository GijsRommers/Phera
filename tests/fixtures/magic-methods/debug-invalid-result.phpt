--TEST--
Dumping fails fatally for a wrong __debugInfo result
--FILE--
<?php
class Report { function __debugInfo() { return 7; } }
try { var_dump(new Report); } catch (Throwable $e) { echo "caught"; }
--EXPECTF--
Fatal error: __debuginfo() must return an array in %s on line 3
