--TEST--
enums: from named argument
--FILE--
<?php
enum E: int{case A=1;} var_dump(E::from(value: 1));
--EXPECT--
enum(E::A)
