--TEST--
if
--FILE--
<?php
$a=2;if($a==1)echo 1;elseif($a==2)echo 2;else echo 3;
--EXPECT--
2