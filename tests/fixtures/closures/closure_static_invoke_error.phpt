--TEST--
closure static invoke error
--FILE--
<?php
try{Closure::__invoke();}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
Call to undefined method Closure::__invoke()
