--TEST--
enums: enum no dynamic property
--FILE--
<?php
enum E{case A;} $x=E::A; try{$x->other=1;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot create dynamic property E::$other
