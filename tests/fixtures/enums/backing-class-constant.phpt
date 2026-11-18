--TEST--
enums: backing class constant
--FILE--
<?php
enum E: int{const V=4;case A=self::V;} echo E::A->value;
--EXPECT--
4
