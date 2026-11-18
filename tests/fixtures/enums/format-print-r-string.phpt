--TEST--
enums: format print r string
--FILE--
<?php
enum E: string{case A="a";} print_r(E::A);
--EXPECT--
E Enum:string
(
    [name] => A
    [value] => a
)
