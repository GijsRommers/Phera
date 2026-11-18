--TEST--
enums: backed cases order
--FILE--
<?php
enum E: int{case B=10;case A=1;} foreach(E::cases() as $case){echo $case->value," ";} echo "end";
--EXPECT--
10 1 end
