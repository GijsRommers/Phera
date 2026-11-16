--TEST--
Class members: typed-float
--FILE--
<?php class Foo {public float $x;} $a=new Foo; $a->x=2; var_dump($a->x);
--EXPECT--
float(2)
