--TEST--
enums: duplicate case constant
--FILE--
<?php
enum E{case A;const A=1;}
--EXPECTF--
Fatal error: Cannot redefine class constant E::A in /private%s on line 2
