--TEST--
enums: cannot clone enum
--FILE--
<?php
enum E{case A;} try{clone E::A;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Trying to clone an uncloneable object of class E
