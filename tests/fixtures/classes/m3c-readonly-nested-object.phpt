--TEST--
Class conformance: readonly-nested-object
--FILE--
<?php class Inner {public int $value=1;} class A {public readonly Inner $inner;function __construct(){$this->inner=new Inner;}}$a=new A;$a->inner->value=7;echo $a->inner->value;
--EXPECT--
7
