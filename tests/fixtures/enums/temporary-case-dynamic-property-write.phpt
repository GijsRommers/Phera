--TEST--
enums: temporary case dynamic property write
--FILE--
<?php
enum E{case A;} E::A->other=1;
--EXPECTF--
Fatal error: Cannot use temporary expression in write context in %s on line 2
