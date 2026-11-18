--TEST--
Undeclared variable remains undefined
--FILE--
<?php
function f() { eval('echo $missing;'); }
f();
--EXPECTF--

Warning: Undefined variable $missing in %s/undeclared-variable-remains-undefined.php(2) : eval()'d code on line 1
