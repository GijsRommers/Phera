--TEST--
Call window: dynamic function in argument
--FILE--
<?php
function f($a,$b){return $a.$b;}function g($x){return $x.'!';}$name='g';echo f($name('a'),$name('b'));
--EXPECT--
a!b!
