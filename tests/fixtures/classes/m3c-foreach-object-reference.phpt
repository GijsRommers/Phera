--TEST--
Class conformance: foreach-object-reference
--FILE--
<?php class Box {public $first=1;public $second=2;} $box=new Box;foreach($box as $key=>&$value){$value*=2;echo $key,"=",$value,"\n";}unset($value);var_dump($box);
--EXPECT--
first=2
second=4
object(Box)#1 (2) {
  ["first"]=>
  int(2)
  ["second"]=>
  int(4)
}
