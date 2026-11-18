--TEST--
traits: direct trait constant
--FILE--
<?php
trait T{const X=2;} try{echo T::X;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot access trait constant T::X directly
