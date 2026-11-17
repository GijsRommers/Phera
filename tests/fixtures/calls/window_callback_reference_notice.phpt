--TEST--
Call window: callback reference notice
--FILE--
<?php
function f(&$x){echo $x;}call_user_func('f',4);
--EXPECTF--

Warning: f(): Argument #1 ($x) must be passed by reference, value given in %s on line 2
4
