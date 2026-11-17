--TEST--
closure lexical class
--FILE--
<?php
class A{function f(){return fn()=>__CLASS__;}}class B{}$f=(new A)->f();$g=$f->bindTo(null,B::class);var_dump($f(),$g());
--EXPECT--
string(1) "A"
string(1) "A"
