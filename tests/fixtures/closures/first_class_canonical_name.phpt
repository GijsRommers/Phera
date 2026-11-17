--TEST--
first class canonical name
--FILE--
<?php
function MiXeD(){};$f=mixed(...);$g=STRLEN(...);
is_callable($f,false,$name);var_dump($name);is_callable($g,false,$name);var_dump($name);
--EXPECT--
string(5) "MiXeD"
string(6) "strlen"
