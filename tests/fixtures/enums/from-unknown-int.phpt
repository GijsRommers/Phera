--TEST--
enums: from unknown int
--FILE--
<?php
enum E: int{case A=7;} try{E::from(8);}catch(ValueError $e){echo $e->getMessage();}
--EXPECT--
8 is not a valid backing value for enum E
