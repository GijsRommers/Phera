--TEST--
Performance cache private_slots
--FILE--
<?php
class A {private $x=1; function read(){return $this->x;} function inc(){$this->x++;}} class B extends A {private $x=10; function child(){return $this->x;}} $b=new B; echo $b->read(), ':', $b->child(), "\n"; $b->inc();echo $b->read(), ':', $b->child(), "\n";
--EXPECT--
1:10
2:10
