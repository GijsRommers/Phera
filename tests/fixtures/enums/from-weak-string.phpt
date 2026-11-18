--TEST--
enums: from weak string
--FILE--
<?php
enum E: string{case A="7";} var_dump(E::from(7)===E::A);
--EXPECT--
bool(true)
