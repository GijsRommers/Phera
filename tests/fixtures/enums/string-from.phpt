--TEST--
enums: string from
--FILE--
<?php
enum E: string{case A="a";} var_dump(E::from("a")===E::A);
--EXPECT--
bool(true)
