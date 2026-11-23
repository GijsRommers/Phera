--TEST--
PHP warns about __invoke visibility and invocation still works
--FILE--
<?php
class Invokable { private function __invoke() { echo "called"; } }
$o = new Invokable;
var_dump(is_callable($o));
$o();
--EXPECTF--
Warning: The magic method Invokable::__invoke() must have public visibility in %s on line 2
bool(true)
called
