--TEST--
enums: temporary case name write
--FILE--
<?php
enum E{case A;} E::A->name="X";
--EXPECTF--
Fatal error: Cannot use temporary expression in write context in %s on line 2
