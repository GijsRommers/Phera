--TEST--
Exceptions: included_throw
--FILE--
<?php
$value=1;try{include __DIR__."/included_exception.inc";}catch(Throwable $error){echo $value,"|",$error->getMessage(),"|",basename($error->getFile()),"|",$error->getLine(),"\n",$error->getTraceAsString();}
--EXPECTF--
2|included|included_exception.inc|3
#0 %s(2): include()
#1 {main}