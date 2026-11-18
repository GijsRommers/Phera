--TEST--
enums: enum instanceof
--FILE--
<?php
enum E {case A;} var_dump(E::A instanceof E);
--EXPECT--
bool(true)
