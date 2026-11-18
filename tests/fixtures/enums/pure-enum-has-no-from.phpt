--TEST--
enums: pure enum has no from
--FILE--
<?php
enum E{case A;} try{E::from(1);}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Call to undefined method E::from()
