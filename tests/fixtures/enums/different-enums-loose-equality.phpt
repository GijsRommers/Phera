--TEST--
enums: different enums loose equality
--FILE--
<?php
enum E {case A;} enum F {case A;} var_dump(E::A==F::A);
--EXPECT--
bool(false)
