--TEST--
Class members: object-array-path
--FILE--
<?php $a=new stdClass; $a->x=new stdClass; $a->x->y=[1]; $a->x->y[]=2; var_dump($a);
--EXPECT--
object(stdClass)#1 (1) {
  ["x"]=>
  object(stdClass)#2 (1) {
    ["y"]=>
    array(2) {
      [0]=>
      int(1)
      [1]=>
      int(2)
    }
  }
}
