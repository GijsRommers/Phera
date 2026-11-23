--TEST--
enums: cases array order
--FILE--
<?php
enum E{case B;case A;case Z;} foreach(E::cases() as $case){echo $case->name;}
--EXPECT--
BAZ
