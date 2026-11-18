--TEST--
enums: pure enum is not backed
--FILE--
<?php
enum E {case A;} var_dump(E::A instanceof BackedEnum);
--EXPECT--
bool(false)
