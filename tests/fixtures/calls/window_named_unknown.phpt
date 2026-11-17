--TEST--
Call window: named unknown
--FILE--
<?php
function f($a=1){}try{f(unknown:2);}catch(Throwable $e){echo $e->getMessage();}
--EXPECT--
Unknown named parameter $unknown
