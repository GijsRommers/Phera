--TEST--
Short getter and setter bodies read and normalize backing storage
--FILE--
<?php
class Person {
    public string $name = 'unknown' {
        get => strtoupper($this->name);
        set => trim($value);
    }
}
$p = new Person;
echo $p->name, "\n";
$p->name = ' Gijs ';
echo $p->name, "\n";
var_dump((array) $p);
--EXPECT--
UNKNOWN
GIJS
array(1) {
  ["name"]=>
  string(4) "Gijs"
}
