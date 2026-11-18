--TEST--
enums: pure case
--FILE--
<?php
enum E {case A;} var_dump(E::A);
--EXPECT--
enum(E::A)
