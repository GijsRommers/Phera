--TEST--
enums: case in class constant
--FILE--
<?php
enum E{case A;} class C{const VALUE=E::A;} var_dump(C::VALUE===E::A);
--EXPECT--
bool(true)
