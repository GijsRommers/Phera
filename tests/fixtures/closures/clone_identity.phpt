--TEST--
clone identity
--FILE--
<?php
$f=function(){};$g=clone $f;var_dump($f==$f,$f==$g,$f===$g,$f!=function(){});
--EXPECT--
bool(true)
bool(false)
bool(false)
bool(true)
