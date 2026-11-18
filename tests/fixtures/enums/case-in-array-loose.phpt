--TEST--
enums: case in array loose
--FILE--
<?php
enum E{case A;case B;} var_dump(in_array(E::A,[E::B]));
--EXPECT--
bool(false)
