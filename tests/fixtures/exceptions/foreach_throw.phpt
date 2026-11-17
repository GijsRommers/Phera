--TEST--
Exceptions: foreach_throw
--FILE--
<?php
try {foreach([1,2,3] as $number){echo $number; if($number==2)throw new Exception("stop");}} catch(Throwable $error){echo $error->getMessage();} foreach([4,5] as $number)echo $number;
--EXPECTF--
12stop45