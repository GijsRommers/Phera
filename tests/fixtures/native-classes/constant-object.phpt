--TEST--
Native classes: constant-object
--FILE--
<?php
class Child extends PDO {
 public function __construct() {
}
 }
 $p=new Child;
 var_dump($p::FETCH_ASSOC);

--EXPECT--
int(2)
