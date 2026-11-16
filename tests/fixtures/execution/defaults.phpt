--TEST--
defaults
--FILE--
<?php
function f($x=3,$y=4){return $x+$y;}echo f(),f(1);
--EXPECT--
75