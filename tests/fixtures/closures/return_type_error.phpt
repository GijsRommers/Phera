--TEST--
return type error
--FILE--
<?php
$f=function():int{return [];};try{$f();}catch(TypeError $e){echo "return error
";}
--EXPECT--
return error
