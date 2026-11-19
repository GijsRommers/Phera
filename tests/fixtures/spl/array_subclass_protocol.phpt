--TEST--
Array container language operations honor overridden methods
--FILE--
<?php
class View extends ArrayIterator {
    public function current(): mixed { return parent::current() * 2; }
}
class Settings extends ArrayObject {
    public function offsetGet(mixed $key): mixed { return parent::offsetGet($key) * 3; }
    public function offsetSet(mixed $key, mixed $value): void {
        echo 'set:', $value, "\n";
        parent::offsetSet($key, $value);
    }
    public function count(): int { return parent::count() + 10; }
}
$a = new Settings([1, 2], iteratorClass: View::class);
$a->append(3);
var_dump($a[1], count($a), iterator_to_array($a));
foreach ($a as $key => $value) {
    echo "$key=$value\n";
}
--EXPECT--
set:3
int(6)
int(13)
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(4)
  [2]=>
  int(6)
}
0=2
1=4
2=6
