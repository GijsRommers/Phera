--TEST--
enums: try from unknown string
--FILE--
<?php
enum E: string{case A="a";} var_dump(E::tryFrom("b"));
--EXPECT--
NULL
