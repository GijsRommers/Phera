--TEST--
enums: duplicate int backing
--FILE--
<?php
enum E: int{case A=1;case B=1;} try{var_dump(E::A);}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Duplicate value in enum E for cases A and B
