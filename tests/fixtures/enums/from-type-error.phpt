--TEST--
enums: from type error
--FILE--
<?php
enum E: string{case A="a";} try{E::from([]);}catch(TypeError $e){echo $e->getMessage();}
--EXPECT--
E::from(): Argument #1 ($value) must be of type string|int, array given
