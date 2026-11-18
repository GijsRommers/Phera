--TEST--
enums: readonly unset name
--FILE--
<?php
enum E{case A;} try{$x=E::A;unset($x->name);}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot unset readonly property E::$name
