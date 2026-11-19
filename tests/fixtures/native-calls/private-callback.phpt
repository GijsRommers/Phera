--TEST--
A native callback sees the calling scope for a private method
--FILE--
<?php
class Counter {
    private function priv(int $value): int {
        return $value + 1;
    }

    public function apply(array $values): array {
        return array_map([$this, 'priv'], $values);
    }
}
$counter = new Counter();
var_dump($counter->apply([1, 2]));
try {
    array_map([$counter, 'priv'], [1, 2]);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
array(2) {
  [0]=>
  int(2)
  [1]=>
  int(3)
}
array_map(): Argument #1 ($callback) must be a valid callback or null, cannot access private method Counter::priv()
