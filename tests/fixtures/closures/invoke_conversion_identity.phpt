--TEST--
invoke conversion identity
--FILE--
<?php
function f($x){return $x;}
foreach([fn()=>1,f(...),strlen(...)] as $a){
$b=Closure::fromCallable([$a,'__invoke']);$c=$a->__invoke(...);
var_dump($a===$b,$a===$c);
}
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
