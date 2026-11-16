--TEST--
Application class sweep: registry
--FILE--
<?php
class Registry {
private static array $items=[];
static function set(
$key,
$value)
{
self::$items[$key]=$value;
}
static function get(
$key)
{
return self::$items[$key];
}
static function all(){
return self::$items;
}
}
 Registry::set('one',
1);
Registry::set('two',
2);
Registry::set('one',
3);
foreach(Registry::all() as $key=>$value){
echo $key,
'=',
$value,
"\n";
}
echo Registry::get('one');

--EXPECT--
one=3
two=2
3
