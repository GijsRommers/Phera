--TEST--
first class default
--FILE--
<?php
function f(Closure $callback=strlen(...)){return $callback("hello");}var_dump(f());
--EXPECT--
int(5)
