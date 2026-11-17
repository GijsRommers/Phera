--TEST--
app sorting helpers
--FILE--
<?php
class Sorter {
    public function decorate($items, callable $key) {
        return array_map(fn($item) => [$key($item), $item], $items);
    }
    public function sort($items, callable $key) {
        $rows = $this->decorate($items, $key);
        for ($i = 0; $i < count($rows); ++$i) {
            for ($j = $i + 1; $j < count($rows); ++$j) {
                if ($rows[$i][0] > $rows[$j][0]) {
                    $temporary = $rows[$i];
                    $rows[$i] = $rows[$j];
                    $rows[$j] = $temporary;
                }
            }
        }
        return array_map(fn($row) => $row[1], $rows);
    }
}
$sorter = new Sorter;
$items = ['pear', 'fig', 'apple', 'plum'];
$sorted = call_user_func_array([$sorter, 'sort'], [$items, strlen(...)]);
var_dump($sorted);
$prefix = 'fruit:';
var_dump(array_map(fn($item) => $prefix . strtoupper($item), $sorted));
try {
    $sorter->sort($items, function ($item) {
        if ($item === 'fig') {
            throw new RuntimeException('invalid key');
        }
        return strlen($item);
    });
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
array(4) {
  [0]=>
  string(3) "fig"
  [1]=>
  string(4) "pear"
  [2]=>
  string(4) "plum"
  [3]=>
  string(5) "apple"
}
array(4) {
  [0]=>
  string(9) "fruit:FIG"
  [1]=>
  string(10) "fruit:PEAR"
  [2]=>
  string(10) "fruit:PLUM"
  [3]=>
  string(11) "fruit:APPLE"
}
invalid key
