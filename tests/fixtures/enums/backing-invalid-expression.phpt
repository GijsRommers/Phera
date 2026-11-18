--TEST--
enums: backing invalid expression
--FILE--
<?php
enum E: int{case A=strlen("f");}
--EXPECTF--
Fatal error: Constant expression contains invalid operations in %s on line 2
