--TEST--
enums: case in instance property default
--FILE--
<?php
enum E{case A;} class C{public E $x=E::A;} var_dump((new C)->x===E::A);
--EXPECT--
bool(true)
