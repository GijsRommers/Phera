--TEST--
app callback properties
--FILE--
<?php
class Actions {
    public $before;
    public $operations = [];
    public function run($name, $value) {
        $before = $this->before;
        $operation = $this->operations[$name];
        $value = $before($value);
        return $operation($value);
    }
}
$actions = new Actions;
$history = [];
$actions->before = function ($value) use (&$history) {
    $history[] = $value;
    return trim($value);
};
$actions->operations = [
    'upper' => strtoupper(...),
    'length' => fn($value) => strlen($value),
    'fail' => function ($value) {
        throw new RuntimeException('bad:' . $value);
    },
];
echo $actions->run('upper', ' ada '), "\n";
var_dump($actions->run('length', ' bob '));
try {
    $actions->run('fail', ' cy ');
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
var_dump($history);
$copy = clone $actions;
var_dump($copy->operations['upper'] === $actions->operations['upper']);
--EXPECT--
ADA
int(3)
bad:cy
array(3) {
  [0]=>
  string(5) " ada "
  [1]=>
  string(5) " bob "
  [2]=>
  string(4) " cy "
}
bool(true)
