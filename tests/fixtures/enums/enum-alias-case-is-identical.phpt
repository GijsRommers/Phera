--TEST--
enums: enum alias case is identical
--FILE--
<?php
enum Original {case A;}
class_alias(Original::class, "Alternate");
var_dump(enum_exists("Alternate"), class_exists("Alternate"), Alternate::A === Original::A, Alternate::A instanceof Alternate);
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
