--TEST--
enums: duplicate case
--FILE--
<?php
enum E{case A;case A;}
--EXPECTF--
Fatal error: Cannot redefine class constant E::A in /private%s on line 2
