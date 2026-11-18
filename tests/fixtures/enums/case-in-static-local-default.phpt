--TEST--
enums: case in static local default
--FILE--
<?php
enum E{case A;} function x(){static $v=E::A;return $v;} var_dump(x()===E::A);
--EXPECT--
bool(true)
