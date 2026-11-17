--TEST--
Class members: foreach-visible
--FILE--
<?php class Foo {public $x=1;private $y=2;} foreach(new Foo as $name=>$value) echo $name,$value;
--EXPECT--
x1