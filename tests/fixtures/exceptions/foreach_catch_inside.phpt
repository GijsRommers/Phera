--TEST--
Exceptions: foreach_catch_inside
--FILE--
<?php
foreach([1,2,3] as $number){try {throw new Exception("x");} catch(Throwable){echo $number;}}
--EXPECTF--
123