--TEST--
static no this error
--FILE--
<?php
class Box{function f(){return static function(){return $this;};}}$f=(new Box)->f();try{$f();}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
Using $this when not in object context
