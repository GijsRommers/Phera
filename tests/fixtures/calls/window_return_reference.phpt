--TEST--
Call window: return reference
--FILE--
<?php
function &f(&$x){return $x;} $x=3;$y=&f($x);$y++;var_dump($x);
--EXPECT--
int(4)
