--TEST--
enums: implements custom interface
--FILE--
<?php
interface I{function label():string;} enum E implements I{case A;function label():string{return "A";}} var_dump(E::A instanceof I);
--EXPECT--
bool(true)
