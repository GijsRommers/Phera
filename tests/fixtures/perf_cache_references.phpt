--TEST--
Performance cache references
--FILE--
<?php
class A {public int $x=1;function get():int{return $this->x;}} $a=new A;function inc($a){$a->x++;echo $a->get(),"\n";} inc($a);$r=&$a->x;inc($a);echo $r,"\n";$r=9;inc($a);
--EXPECT--
2
3
3
10
