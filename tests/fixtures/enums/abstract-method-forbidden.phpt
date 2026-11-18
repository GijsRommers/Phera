--TEST--
enums: abstract method forbidden
--FILE--
<?php
enum E{abstract function x();}
--EXPECTF--
Fatal error: Enum method E::x() must not be abstract in %s on line 2
