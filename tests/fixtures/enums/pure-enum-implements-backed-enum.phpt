--TEST--
enums: pure enum implements backed enum
--FILE--
<?php
enum E implements BackedEnum{case A;}
--EXPECTF--
Fatal error: Non-backed enum E cannot implement interface BackedEnum in %s on line 2
