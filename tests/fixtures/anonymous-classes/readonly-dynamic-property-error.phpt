--TEST--
anonymous-classes: readonly dynamic property error
--FILE--
<?php
$x=new readonly class{}; try{$x->v=1;}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Cannot create dynamic property class@anonymous::$v
