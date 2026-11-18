--TEST--
enums: backing type error
--FILE--
<?php
enum E: int{case A="1";} try{var_dump(E::A);}catch(TypeError $e){echo $e->getMessage();}
--EXPECT--
Enum case type string does not match enum backing type int
