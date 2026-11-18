--TEST--
enums: from unknown string
--FILE--
<?php
enum E: string{case A="a";} try{E::from("b");}catch(ValueError $e){echo $e->getMessage();}
--EXPECT--
"b" is not a valid backing value for enum E
