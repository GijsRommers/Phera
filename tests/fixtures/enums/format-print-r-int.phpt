--TEST--
enums: format print r int
--FILE--
<?php
enum E: int{case A=3;} print_r(E::A);
--EXPECT--
E Enum:int
(
    [name] => A
    [value] => 3
)
