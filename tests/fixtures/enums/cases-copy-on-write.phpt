--TEST--
enums: cases copy on write
--FILE--
<?php
enum E{case A;case B;} $x=E::cases();unset($x[0]);var_dump(count(E::cases()));
--EXPECT--
int(2)
