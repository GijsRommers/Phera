--TEST--
enums: pure no value property
--FILE--
<?php
enum E{case A;} var_dump(isset(E::A->value));
--EXPECT--
bool(false)
