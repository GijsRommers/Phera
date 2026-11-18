--TEST--
enums: format var dump backed
--FILE--
<?php
enum E: string{case A="a";} var_dump(E::A);
--EXPECT--
enum(E::A)
