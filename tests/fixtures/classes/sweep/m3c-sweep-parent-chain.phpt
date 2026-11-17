--TEST--
Application class sweep: parent-chain
--FILE--
<?php
class Root {
function label(
)
{
return 'root';
}
 static function make(
)
{
return new static;
}
}
 class Middle extends Root {
function label(){
return parent::label().'/middle';
}
}
 class Leaf extends Middle {
function label(){
return parent::label().'/leaf';
}
 static function build(){
return parent::make();
}
}
 $leaf=Leaf::build();
echo $leaf->label(),
"\n",
get_class($leaf),
"\n";

--EXPECT--
root/middle/leaf
Leaf
