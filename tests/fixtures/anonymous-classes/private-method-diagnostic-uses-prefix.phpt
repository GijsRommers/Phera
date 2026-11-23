--TEST--
anonymous-classes: private method diagnostic uses prefix
--FILE--
<?php
$x=new class{private function f(){}};try{$x->f();}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Call to private method class@anonymous::f() from global scope
