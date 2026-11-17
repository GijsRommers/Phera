--TEST--
Native classes: constant-object-expression
--FILE--
<?php
class Child extends PDO {
 public function __construct() {
}
 }
 var_dump((new Child)::FETCH_NUM);

--EXPECT--
int(3)
