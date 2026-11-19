--TEST--
Foreach by reference retains the cell returned by a reference getter
--FILE--
<?php
class C {
    private array $items = [1];
    public array $exposed { &get => $this->items; }
}
$c = new C;
foreach ($c as &$value) {
    $value[] = 2;
}
var_dump($c->exposed);
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
