--TEST--
enums: index backing string in constant expression
--FILE--
<?php
enum E:string {case A="hi";case B=self::A->value[0];} var_dump(E::B);
--EXPECT--
enum(E::B)
