--TEST--
static closure default
--FILE--
<?php
function f(Closure $callback=static function($x){return $x*2;}){return $callback(3);}var_dump(f());
--EXPECT--
int(6)
