--TEST--
traits: matching array property
--FILE--
<?php
trait T {public $p=["a"=>1];} class C{use T; public $p=["a"=>1];} var_dump((new C)->p);
--EXPECT--
array(1) {
  ["a"]=>
  int(1)
}
