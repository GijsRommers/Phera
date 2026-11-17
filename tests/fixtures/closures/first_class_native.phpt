--TEST--
first class native
--FILE--
<?php
$f=strlen(...);var_dump($f instanceof Closure,$f("hello"));
--EXPECT--
bool(true)
int(5)
