--TEST--
parameters strict
--FILE--
<?php
declare(strict_types=1);$f=function(int $x):int{return $x;};try{$f("4");}catch(TypeError $e){echo "type error
";}
--EXPECT--
type error
