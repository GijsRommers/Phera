--TEST--
lexical clone equality
--FILE--
<?php
function f(){return fn()=>1;}$a=f();var_dump($a==clone $a,$a==f(),$a===$a);
--EXPECT--
bool(false)
bool(false)
bool(true)
