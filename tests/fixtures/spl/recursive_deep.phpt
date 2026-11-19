--TEST--
Recursive iteration visits a leaf nested in 100,000 arrays
--FILE--
<?php
$value = 17;
for ($depth = 0; $depth < 100000; ++$depth) {
    $value = [$value];
}
$rows = new RecursiveIteratorIterator(new RecursiveArrayIterator($value));
foreach ($rows as $leaf) {
    echo $rows->getDepth(), '=', $leaf, "\n";
}
--EXPECT--
99999=17
