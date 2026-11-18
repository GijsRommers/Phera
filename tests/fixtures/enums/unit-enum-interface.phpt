--TEST--
enums: unit enum interface
--FILE--
<?php
enum E {case A;} var_dump(E::A instanceof UnitEnum);
--EXPECT--
bool(true)
