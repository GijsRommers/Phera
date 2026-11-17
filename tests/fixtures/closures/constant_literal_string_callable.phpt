--TEST--
constant literal string callable
--FILE--
<?php
class C{public $f=('strlen')(...);}
$c=new C;var_dump(($c->f)('abc'));
function f($callback=('strrev')(...)){return $callback('abc');}var_dump(f());
--EXPECT--
int(3)
string(3) "cba"
