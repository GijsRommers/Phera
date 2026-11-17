--TEST--
bind function receiver
--FILE--
<?php
function f(){return isset($this);}$f=Closure::fromCallable("f");$g=$f->bindTo(new stdClass);var_dump($g());
--EXPECT--
bool(true)
