--TEST--
Exceptions: throw_coalesce
--FILE--
<?php
$value=null; try {$result=$value ?? throw new Exception("missing");} catch(Throwable $error){echo $error->getMessage();}
--EXPECTF--
missing