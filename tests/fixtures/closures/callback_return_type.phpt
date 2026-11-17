--TEST--
callback return type
--FILE--
<?php
function callback():callable{return fn()=>8;}$f=callback();var_dump($f());
--EXPECT--
int(8)
