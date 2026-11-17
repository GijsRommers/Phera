--TEST--
bind scope bad type
--FILE--
<?php
$f=fn()=>1;try{Closure::bind($f,null,[]);}catch(TypeError $e){echo $e->getMessage(),"
";}
--EXPECT--
Closure::bind(): Argument #3 ($newScope) must be of type object|string|null, array given
