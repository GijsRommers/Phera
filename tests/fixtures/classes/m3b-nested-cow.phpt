--TEST--
Class members: nested-cow
--FILE--
<?php class Foo {public $x=[];} $a=new Foo; $copy=$a->x; $a->x[]=1; var_dump($copy,$a->x);
--EXPECT--
array(0) {
}
array(1) {
  [0]=>
  int(1)
}
