--TEST--
arrow deep nested
--FILE--
<?php
$x=5;$f=fn()=>fn()=>fn()=>fn()=>$x;$g=$f()()();var_dump($g());
--EXPECT--
int(5)
