--TEST--
arrow superglobal
--FILE--
<?php
$_GET=["value"=>1];$f=fn()=>$_GET["value"];$_GET["value"]=2;var_dump($f());
--EXPECT--
int(2)
