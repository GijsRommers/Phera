--TEST--
ArrayObject distinguishes storage, standard properties and debug views
--FILE--
<?php
class Settings extends ArrayObject {
    public string $name = 'property';
}
$a = new Settings(['name' => 'storage', 'value' => null], ArrayObject::ARRAY_AS_PROPS);
$a->extra = 42;
var_dump($a->name, isset($a['value']), $a->offsetExists('value'), $a);
var_dump((array) $a, json_encode($a), $a == clone $a);
$a->setFlags(ArrayObject::STD_PROP_LIST);
var_dump((array) $a, json_encode($a), get_object_vars($a));
--EXPECT--
string(8) "property"
bool(false)
bool(true)
object(Settings)#1 (2) {
  ["name"]=>
  string(8) "property"
  ["storage":"ArrayObject":private]=>
  array(3) {
    ["name"]=>
    string(7) "storage"
    ["value"]=>
    NULL
    ["extra"]=>
    int(42)
  }
}
array(3) {
  ["name"]=>
  string(7) "storage"
  ["value"]=>
  NULL
  ["extra"]=>
  int(42)
}
string(42) "{"name":"storage","value":null,"extra":42}"
bool(true)
array(1) {
  ["name"]=>
  string(8) "property"
}
string(19) "{"name":"property"}"
array(1) {
  ["name"]=>
  string(8) "property"
}
