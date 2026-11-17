--TEST--
first class native closure identity
--FILE--
<?php
$f=strlen(...);$g=$f(...);var_dump($f===$g,$g("hello"));
--EXPECT--
bool(true)
int(5)
