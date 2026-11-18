--TEST--
enums: from strict string
--FILE--
<?php
declare(strict_types=1); enum E: string{case A="7";} try{E::from(7);}catch(TypeError $e){echo $e->getMessage();}
--EXPECT--
E::from(): Argument #1 ($value) must be of type string, int given
