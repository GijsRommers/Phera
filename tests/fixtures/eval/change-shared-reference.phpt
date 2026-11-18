--TEST--
Change shared reference
--FILE--
<?php
function f(&$x) { eval('$x += 4;'); } $x=3; f($x); var_dump($x);
--EXPECT--
int(7)
