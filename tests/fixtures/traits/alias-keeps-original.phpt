--TEST--
traits: alias keeps original
--FILE--
<?php
trait T{function m(){return 3;}} class C{use T {m as n;}} $x=new C; echo $x->m(),$x->n();
--EXPECT--
33
