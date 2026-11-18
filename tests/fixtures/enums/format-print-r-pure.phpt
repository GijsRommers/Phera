--TEST--
enums: format print r pure
--FILE--
<?php
enum E{case A;} print_r(E::A);
--EXPECT--
E Enum
(
    [name] => A
)
