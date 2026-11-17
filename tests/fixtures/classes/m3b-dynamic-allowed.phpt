--TEST--
Class members: dynamic-allowed
--FILE--
<?php #[AllowDynamicProperties] class Foo {} $a=new Foo; $a->x=1; var_dump($a);
--EXPECT--
object(Foo)#1 (1) {
  ["x"]=>
  int(1)
}
