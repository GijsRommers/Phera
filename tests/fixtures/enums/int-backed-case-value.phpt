--TEST--
enums: int backed case value
--FILE--
<?php
enum E: int {case A=7;} var_dump(E::A->value);
--EXPECT--
int(7)
