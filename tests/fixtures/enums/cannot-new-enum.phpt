--TEST--
enums: cannot new enum
--FILE--
<?php
enum E{case A;} try{new E;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot instantiate enum E
