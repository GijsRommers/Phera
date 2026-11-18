--TEST--
enums: readonly name
--FILE--
<?php
enum E{case A;} $x=E::A; try{$x->name="X";}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot modify readonly property E::$name
