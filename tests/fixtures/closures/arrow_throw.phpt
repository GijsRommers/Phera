--TEST--
arrow throw
--FILE--
<?php
$f=fn()=>throw new RuntimeException("arrow");try{$f();}catch(RuntimeException $e){echo $e->getMessage(),"
";}
--EXPECT--
arrow
