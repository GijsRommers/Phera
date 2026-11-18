--TEST--
enums: case in match result
--FILE--
<?php
enum E{case A;} var_dump(match(1){1=>E::A}===E::A);
--EXPECT--
bool(true)
