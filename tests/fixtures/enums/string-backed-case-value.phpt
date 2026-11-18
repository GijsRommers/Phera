--TEST--
enums: string backed case value
--FILE--
<?php
enum E: string {case A="aa";} var_dump(E::A->value);
--EXPECT--
string(2) "aa"
