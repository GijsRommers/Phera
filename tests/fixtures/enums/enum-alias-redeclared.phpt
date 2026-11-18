--TEST--
enums: enum alias redeclared
--FILE--
<?php
enum Original {case A;}
class_alias(Original::class, "Alternate");
class_alias(Original::class, "Alternate");
--EXPECTF--
Warning: Cannot redeclare enum Alternate (previously declared in %s:2) in %s on line 4
