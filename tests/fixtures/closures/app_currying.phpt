--TEST--
app currying
--FILE--
<?php
class Formatter {
    public function __construct(private $separator) {}
    public function prefix($prefix) {
        return function ($suffix) use ($prefix) {
            return fn($value) => $prefix . $this->separator . $value . $suffix;
        };
    }
}
function curry(callable $operation) {
    return fn($left) => fn($right) => $operation($left, $right);
}
$formatter = new Formatter(':');
$stage = $formatter->prefix('item');
$format = $stage('!');
$values = ['one', 'two', 'three'];
var_dump(array_map($format, $values));
$add = curry(fn($a, $b) => $a + $b);
$addTen = $add(10);
var_dump(array_map($addTen, [1, 2, 3]));
$divide = curry(function ($a, $b) {
    if ($b === 0) {
        throw new RuntimeException('zero');
    }
    return $a / $b;
});
$divideTen = $divide(10);
try {
    var_dump($divideTen(2));
    $divideTen(0);
} catch (RuntimeException $error) {
    echo strtoupper($error->getMessage()), "\n";
}
--EXPECT--
array(3) {
  [0]=>
  string(9) "item:one!"
  [1]=>
  string(9) "item:two!"
  [2]=>
  string(11) "item:three!"
}
array(3) {
  [0]=>
  int(11)
  [1]=>
  int(12)
  [2]=>
  int(13)
}
int(5)
ZERO
