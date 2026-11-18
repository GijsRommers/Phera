--TEST--
traits: trait is not an instance type
--FILE--
<?php
trait T{} class C{use T;} var_dump(new C instanceof T);
--EXPECT--
bool(false)
