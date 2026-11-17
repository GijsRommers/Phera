--TEST--
first class reference
--FILE--
<?php
function &same(&$x){return $x;}$g=same(...);$x=3;$y=&$g($x);$y=9;var_dump($x);
--EXPECT--
int(9)
