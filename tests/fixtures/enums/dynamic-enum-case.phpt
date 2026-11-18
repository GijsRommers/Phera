--TEST--
enums: dynamic enum case
--FILE--
<?php
enum E{case A;} $case="A"; var_dump(E::{$case}===E::A);
--EXPECT--
bool(true)
