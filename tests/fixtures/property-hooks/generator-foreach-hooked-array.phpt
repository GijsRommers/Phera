--TEST--
A generator can iterate the value returned by an array getter
--FILE--
<?php
class Items {
    public array $items {
        get {
            echo "get items\n";
            return [3, 5];
        }
    }
    public function values(): Generator {
        foreach ($this->items as $item) {
            yield $item * 2;
        }
    }
}
$values = (new Items())->values();
echo "created\n";
var_dump(iterator_to_array($values));
--EXPECT--
created
get items
array(2) {
  [0]=>
  int(6)
  [1]=>
  int(10)
}
