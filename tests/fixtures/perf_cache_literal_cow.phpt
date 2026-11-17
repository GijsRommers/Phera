--TEST--
Performance cache literal_cow
--FILE--
<?php
class A {public $x=2; public $a=[1,2];} $a=new A;$b=new A;$a->a[]=3;$a->x++;echo count($a->a),':',count($b->a),':',$a->x,':',$b->x,"\n";
--EXPECT--
3:2:3:2
