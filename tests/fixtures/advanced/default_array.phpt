--TEST--
default_array
--FILE--
<?php
function f($a=[]){echo count($a);}f();
--EXPECT--
0