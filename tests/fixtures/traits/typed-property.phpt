--TEST--
traits: typed property
--FILE--
<?php
trait T { public int $i = 10;} class C {use T;} $c=new C; $c->i=12; echo $c->i;
--EXPECT--
12
