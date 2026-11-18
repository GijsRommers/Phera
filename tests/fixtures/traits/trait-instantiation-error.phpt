--TEST--
traits: trait instantiation error
--FILE--
<?php
trait T{} try{new T;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot instantiate trait T
