--TEST--
app memoisation
--FILE--
<?php
class Memo {
    public static function wrap(callable $operation) {
        $cache = [];
        return function ($key) use ($operation, &$cache) {
            if (!array_key_exists($key, $cache)) {
                $cache[$key] = $operation($key);
            }
            return $cache[$key];
        };
    }
}
$calls = 0;
$calculate = Memo::wrap(function ($word) use (&$calls) {
    ++$calls;
    if ($word === 'bad') {
        throw new RuntimeException('invalid');
    }
    return strtoupper($word) . ':' . strlen($word);
});
$words = ['ada', 'bob', 'ada', 'cy', 'bob'];
$results = array_map($calculate, $words);
var_dump($results, $calls);
for ($attempt = 0; $attempt < 2; ++$attempt) {
    try {
        $calculate('bad');
    } catch (RuntimeException $error) {
        echo $error->getMessage(), "\n";
    }
}
var_dump($calls);
$other = Memo::wrap(fn($word) => strtolower($word));
var_dump($other('ADA'), $calculate('ada'));
--EXPECT--
array(5) {
  [0]=>
  string(5) "ADA:3"
  [1]=>
  string(5) "BOB:3"
  [2]=>
  string(5) "ADA:3"
  [3]=>
  string(4) "CY:2"
  [4]=>
  string(5) "BOB:3"
}
int(3)
invalid
invalid
int(5)
string(3) "ada"
string(5) "ADA:3"
