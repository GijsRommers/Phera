--TEST--
enums: temporary case increment
--FILE--
<?php
enum E: int{case A=1;} E::A->value++;
--EXPECTF--
Fatal error: Cannot use temporary expression in write context in /private%s on line 2
