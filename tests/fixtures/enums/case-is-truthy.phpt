--TEST--
enums: case is truthy
--FILE--
<?php
enum E {case A;} var_dump(E::A==true,E::A==false);
--EXPECT--
bool(true)
bool(false)
