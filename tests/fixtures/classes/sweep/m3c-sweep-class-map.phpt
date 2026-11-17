--TEST--
Application class sweep: class-map
--FILE--
<?php
class First {
function value(
)
{
return 1;
}
}
class Second {
function value(){
return 2;
}
}
class Locator {
private array $names=['a'=>'First',
'b'=>'Second'];
function get(string $key): object {
$name=$this->names[$key];
return new $name;
}
}
$locator=new Locator;
foreach(['a',
'b',
'a'] as $key){
$object=$locator->get($key);
echo get_class($object),
'=',
$object->value(),
"\n";
}

--EXPECT--
First=1
Second=2
First=1
