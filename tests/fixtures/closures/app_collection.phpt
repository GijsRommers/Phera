--TEST--
app collection
--FILE--
<?php
class Collection {
    public function __construct(private $items) {}
    public function map(callable $callback) {
        return new Collection(array_map($callback, $this->items));
    }
    public function filter(callable $callback) {
        return new Collection(array_filter($this->items, $callback));
    }
    public function values() {
        return array_values($this->items);
    }
}
$people = new Collection([
    ['name' => 'ada', 'age' => 30],
    ['name' => 'bob', 'age' => 17],
    ['name' => 'cy', 'age' => 40],
]);
$minimum = 18;
$adults = $people->filter(fn($person) => $person['age'] >= $minimum);
$names = $adults->map(function ($person) {
    if ($person['name'] === '') {
        throw new RuntimeException('empty');
    }
    return strtoupper($person['name']);
});
var_dump($names->values());
$minimum = 99;
$lengths = $names->map(fn($name) => strlen($name));
var_dump($lengths->values());
try {
    $names->map(function ($name) {
        throw new RuntimeException('stop:' . $name);
    });
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
array(2) {
  [0]=>
  string(3) "ADA"
  [1]=>
  string(2) "CY"
}
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(2)
}
stop:ADA
