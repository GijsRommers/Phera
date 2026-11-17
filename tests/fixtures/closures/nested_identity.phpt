--TEST--
nested identity
--FILE--
<?php
$f=function(){};$g=clone $f;var_dump([$f]==[$g],[$f]==[$f]);
--EXPECT--
bool(false)
bool(true)
