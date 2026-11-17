--TEST--
app stateful factory
--FILE--
<?php
class Counters {
    public function create($label) {
        return function ($step = 1) use ($label) {
            static $count = 0;
            if ($step < 0) {
                throw new RuntimeException('negative');
            }
            $count += $step;
            return strtoupper($label) . ':' . $count;
        };
    }
}
$factory = new Counters;
$first = $factory->create('a');
$second = $factory->create('b');
echo $first(), "\n";
echo $first(2), "\n";
$copy = clone $first;
echo $copy(3), "\n";
echo $first(), "\n";
echo $second(), "\n";
try {
    $copy(-1);
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
$callbacks = [$first, $second, $copy];
$values = array_map(function ($callback) {
    return call_user_func_array($callback, [2]);
}, $callbacks);
var_dump($values);
var_dump($first === $copy, is_callable($copy));
--EXPECT--
A:1
A:3
A:6
A:4
B:1
negative
array(3) {
  [0]=>
  string(3) "A:6"
  [1]=>
  string(3) "B:3"
  [2]=>
  string(3) "A:8"
}
bool(false)
bool(true)
