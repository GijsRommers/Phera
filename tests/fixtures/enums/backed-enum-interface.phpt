--TEST--
enums: backed enum interface
--FILE--
<?php
enum E: int {case A=3;} var_dump(E::A instanceof BackedEnum);
--EXPECT--
bool(true)
