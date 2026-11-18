--TEST--
enums: duplicate string backing
--FILE--
<?php
enum E: string{case A="a";case B="a";} try{E::from("a");}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Duplicate value in enum E for cases A and B
