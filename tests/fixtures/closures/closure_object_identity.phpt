--TEST--
closure object identity
--FILE--
<?php
$f=function(){};$g=$f;$h=clone $f;var_dump($f instanceof Closure,is_object($f),is_callable($f),get_class($f),$f===$g,$f===$h);
--EXPECT--
bool(true)
bool(true)
bool(true)
string(7) "Closure"
bool(true)
bool(false)
