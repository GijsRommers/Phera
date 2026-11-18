--TEST--
enums: case in static property default
--FILE--
<?php
enum E{case A;} class C{public static E $x=E::A;} var_dump(C::$x===E::A);
--EXPECT--
bool(true)
