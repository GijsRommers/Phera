--TEST--
callable name output
--FILE--
<?php
$f=fn()=>1;var_dump(is_callable($f,false,$name),substr($name,0,9),is_callable("strlen",false,$name),$name);
--EXPECT--
bool(true)
string(9) "{closure:"
bool(true)
string(6) "strlen"
