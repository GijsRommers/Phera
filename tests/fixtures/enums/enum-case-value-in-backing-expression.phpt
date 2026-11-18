--TEST--
enums: enum case value in backing expression
--FILE--
<?php
enum E:int{case A=3;case B=self::A->value+1;} echo E::B->value;
--EXPECT--
4
