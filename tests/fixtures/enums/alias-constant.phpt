--TEST--
enums: alias constant
--FILE--
<?php
enum E{case A;const DEFAULT=self::A;} var_dump(E::DEFAULT===E::A);
--EXPECT--
bool(true)
