--TEST--
enums: backing missing expression
--FILE--
<?php
enum E: int{case A;}
--EXPECTF--
Fatal error: Case A of backed enum E must have a value in %s on line 2
