--TEST--
enums: same case loose equality
--FILE--
<?php
enum E {case A;} var_dump(E::A==E::A);
--EXPECT--
bool(true)
