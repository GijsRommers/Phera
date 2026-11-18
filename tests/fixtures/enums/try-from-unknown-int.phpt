--TEST--
enums: try from unknown int
--FILE--
<?php
enum E: int{case A=7;} var_dump(E::tryFrom(8));
--EXPECT--
NULL
