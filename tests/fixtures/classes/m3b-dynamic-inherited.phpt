--TEST--
Class members: dynamic-inherited
--FILE--
<?php #[AllowDynamicProperties] class A {} class B extends A {} $a=new B; $a->x=1; var_dump($a);
--EXPECT--
object(B)#1 (1) {
  ["x"]=>
  int(1)
}
