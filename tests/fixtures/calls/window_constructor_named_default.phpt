--TEST--
Call window: constructor named default
--FILE--
<?php
class A{function __construct(public $a=1,public $b=2){}}$a=new A(b:7);var_dump($a->a,$a->b);
--EXPECT--
int(1)
int(7)
