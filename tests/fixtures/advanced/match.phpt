--TEST--
match
--FILE--
<?php
$a=2;echo match($a){1=>'a',2,3=>'b',default=>'c'};
--EXPECT--
b