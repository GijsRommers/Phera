--TEST--
enums: readonly backing value
--FILE--
<?php
enum E: int{case A=1;} $x=E::A; try{$x->value++;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot modify readonly property E::$value
