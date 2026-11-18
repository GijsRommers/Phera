--TEST--
enums: backing pure forbidden
--FILE--
<?php
enum E{case A=1;}
--EXPECTF--
Fatal error: Case A of non-backed enum E must not have a value in %s on line 2
