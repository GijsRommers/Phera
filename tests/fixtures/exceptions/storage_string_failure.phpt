--TEST--
Exceptions: storage_string_failure
--FILE--
<?php
$text="abc";try{$text[]="d";}catch(Throwable){}echo $text;try{$text[]="d";}catch(Throwable $error){echo "|",$error->getMessage();}
--EXPECTF--
abc|[] operator not supported for strings