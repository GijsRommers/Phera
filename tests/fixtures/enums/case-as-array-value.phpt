--TEST--
enums: case as array value
--FILE--
<?php
enum E{case A;} $items=[E::A]; var_dump($items[0]===E::A);
--EXPECT--
bool(true)
