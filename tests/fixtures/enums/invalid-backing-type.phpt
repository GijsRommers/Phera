--TEST--
enums: invalid backing type
--FILE--
<?php
enum E: bool{case A=true;}
--EXPECTF--
Fatal error: Enum backing type must be int or string, bool given in /private%s on line 2
