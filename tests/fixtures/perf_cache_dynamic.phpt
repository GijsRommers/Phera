--TEST--
Performance cache dynamic
--FILE--
<?php
#[AllowDynamicProperties] class A {} $a=new A;$a->x=4;function read($a){echo $a->x,"\n";} read($a);unset($a->x);$a->x=8;read($a);
--EXPECT--
4
8
