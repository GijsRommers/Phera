--TEST--
enums: case sensitive case names
--FILE--
<?php
enum E{case A;case a;} var_dump(E::A,E::a);
--EXPECT--
enum(E::A)
enum(E::a)
