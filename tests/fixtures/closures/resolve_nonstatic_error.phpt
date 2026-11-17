--TEST--
resolve nonstatic error
--FILE--
<?php
class A{function f(){}}$f="A::f";try{$f();}catch(Error $e){echo $e->getMessage(),"
";}var_dump(is_callable($f));
--EXPECT--
Non-static method A::f() cannot be called statically
bool(false)
