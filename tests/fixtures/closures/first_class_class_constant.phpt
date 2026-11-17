--TEST--
first class class constant
--FILE--
<?php
class A{const CALLBACK=strlen(...);}$f=A::CALLBACK;var_dump($f("hello"));
--EXPECT--
int(5)
