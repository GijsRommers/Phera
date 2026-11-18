--TEST--
enums: case in parameter default
--FILE--
<?php
enum E{case A;} function read(E $x=E::A){return $x;} var_dump(read()===E::A);
--EXPECT--
bool(true)
