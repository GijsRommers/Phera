--TEST--
enums: self referencing enum case
--FILE--
<?php
enum E:int{case A=self::A->value;} try{E::A;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot declare self-referencing constant self::A
