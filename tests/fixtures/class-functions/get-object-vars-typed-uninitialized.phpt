--TEST--
Get object vars typed uninitialized
--FILE--
<?php
class A { public int $unset; public ?int $set = null; } var_dump(get_object_vars(new A));
--EXPECT--
array(1) {
  ["set"]=>
  NULL
}
