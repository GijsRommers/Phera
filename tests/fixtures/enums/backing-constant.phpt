--TEST--
enums: backing constant
--FILE--
<?php
const VALUE=4; enum E: int{case A=VALUE;} echo E::A->value;
--EXPECT--
4
