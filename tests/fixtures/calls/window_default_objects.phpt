--TEST--
Call window: default objects
--FILE--
<?php
class A{public $x=4;}function f($a=new A,$b=2){$a->x++;echo $a->x+$b,"\n";}f();f(b:5);f();
--EXPECT--
7
10
7
