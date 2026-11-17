--TEST--
first class property
--FILE--
<?php
class A{public $callback=strlen(...);}$a=new A;$f=$a->callback;var_dump($f("hello"));
--EXPECT--
int(5)
