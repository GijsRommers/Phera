--TEST--
traits: property default is per instance
--FILE--
<?php
trait T {public $v=1;} class C {use T;} $a=new C; $b=new C; $a->v++; echo $b->v;
--EXPECT--
1
