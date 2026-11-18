--TEST--
enums: cannot extend enum
--FILE--
<?php
enum E{case A;} class C extends E{}
--EXPECTF--
Fatal error: Class C cannot extend enum E in /private%s on line 2
