--TEST--
first class capture in arrow
--FILE--
<?php
$f=fn()=>strlen(...);$g=$f();var_dump($g("abc"));
--EXPECT--
int(3)
