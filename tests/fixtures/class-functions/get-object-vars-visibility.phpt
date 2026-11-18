--TEST--
Get object vars visibility
--FILE--
<?php
class A { private $hidden=1; public $visible=2; } var_dump(get_object_vars(new A));
--EXPECT--
array(1) {
  ["visible"]=>
  int(2)
}
