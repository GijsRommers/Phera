--TEST--
A by-reference getter supports aliases, by-reference calls, and array appends
--FILE--
<?php
class C {
    public array $items = [] { &get => $this->items; }
}
function append(array &$items) { $items[] = 3; }
$c = new C;
$c->items[] = 1;
$alias =& $c->items;
$alias[] = 2;
append($c->items);
var_dump($c->items);
--EXPECT--
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
