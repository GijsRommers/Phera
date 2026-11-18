--TEST--
enums: backing constant expression
--FILE--
<?php
enum E: int{case A=1+2;} echo E::A->value;
--EXPECT--
3
