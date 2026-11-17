--TEST--
closure property error
--FILE--
<?php
$f=fn()=>1;try{$f->value=3;}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
Cannot create dynamic property Closure::$value
