--TEST--
undefined function spelling
--FILE--
<?php
namespace Foo;
try{$f=\Foo\MiSsInG(...);}catch(\Error $e){echo $e->getMessage(),"\n";}
try{MiSsInG();}catch(\Error $e){echo $e->getMessage(),"\n";}
--EXPECT--
Call to undefined function Foo\MiSsInG()
Call to undefined function Foo\MiSsInG()
