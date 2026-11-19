--TEST--
A generator can iterate a reference getter's array by reference
--FILE--
<?php
class Items {
    public array $items = [3, 5] {
        &get {
            echo "get items\n";
            return $this->items;
        }
    }
    public function values(): Generator {
        foreach ($this->items as &$item) {
            $item *= 2;
            yield $item;
        }
    }
}
$items = new Items();
var_dump(iterator_to_array($items->values()), $items->items);
--EXPECT--
get items
get items
array(2) {
  [0]=>
  int(6)
  [1]=>
  int(10)
}
array(2) {
  [0]=>
  int(6)
  [1]=>
  int(10)
}
