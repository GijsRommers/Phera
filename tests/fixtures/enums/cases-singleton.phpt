--TEST--
enums: cases singleton
--FILE--
<?php
enum E{case A;} var_dump(E::cases()[0]===E::A);
--EXPECT--
bool(true)
