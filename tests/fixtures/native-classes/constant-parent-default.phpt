--TEST--
Native classes: constant-parent-default
--FILE--
<?php
class Child extends PDO {
 public $mode=parent::FETCH_ASSOC;
 const MODE=parent::FETCH_NUM;
 public function __construct() {
}
 }
 $p=new Child;
 var_dump($p->mode,Child::MODE);

--EXPECT--
int(2)
int(3)
