--TEST--
Recursive iteration traverses leaves and parents in PHP order
--FILE--
<?php
$data = ['a' => [1, 'b' => [2]], 'c' => 3];
foreach ([0, 1, 2] as $mode) {
    $items = new RecursiveIteratorIterator(new RecursiveArrayIterator($data), $mode);
    foreach ($items as $key => $value) {
        echo $mode, '/', $items->getDepth(), '/', $key, '=';
        echo is_array($value) ? 'array' : $value;
        echo "\n";
    }
}
--EXPECT--
0/1/0=1
0/2/0=2
0/0/c=3
1/0/a=array
1/1/0=1
1/1/b=array
1/2/0=2
1/0/c=3
2/1/0=1
2/2/0=2
2/1/b=array
2/0/a=array
2/0/c=3
