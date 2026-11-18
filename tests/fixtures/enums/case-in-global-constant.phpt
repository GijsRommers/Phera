--TEST--
enums: case in global constant
--FILE--
<?php
enum E{case A;} const VALUE=E::A; var_dump(VALUE===E::A);
--EXPECT--
bool(true)
