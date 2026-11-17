--TEST--
Call window: default exception
--FILE--
<?php
class A{function __construct(){throw new Exception('default');}}function f($x=new A){}try{f();}catch(Throwable $e){echo $e->getMessage();}f(1);echo 'ok';
--EXPECT--
defaultok
