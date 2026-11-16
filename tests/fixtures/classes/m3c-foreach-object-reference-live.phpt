--TEST--
Class conformance: foreach-object-reference-live
--FILE--
<?php class Box {public $first=1;public $second=2;public $third=3;} $box=new Box;foreach($box as $key=>&$value){if($key==="first"){$box->third=9;unset($box->second);}echo $key,"=",$value,"\n";}unset($value);
--EXPECT--
first=1
third=9
