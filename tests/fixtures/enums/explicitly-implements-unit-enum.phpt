--TEST--
enums: explicitly implements unit enum
--FILE--
<?php
enum E implements UnitEnum{case A;}
--EXPECTF--
Fatal error: Enum E cannot implement previously implemented interface UnitEnum in %s on line 2
