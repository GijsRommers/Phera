--TEST--
enums: case in array strict
--FILE--
<?php
enum E{case A;case B;} var_dump(in_array(E::A,[E::B,E::A],true));
--EXPECT--
bool(true)
