--TEST--
enums: backed enum is unit enum
--FILE--
<?php
enum E: string {case A="y";} var_dump(E::A instanceof UnitEnum);
--EXPECT--
bool(true)
