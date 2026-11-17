--TEST--
first class closure identity
--FILE--
<?php
$f=fn($x)=>$x+1;$g=$f(...);var_dump($g===$f,$g(2));
--EXPECT--
bool(true)
int(3)
