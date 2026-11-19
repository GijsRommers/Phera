--TEST--
Array literals, argument lists and match arms of 20,000 entries
--FILE--
<?php
$a = eval('return [' . implode(', ', array_map(fn ($i) => "'k$i' => $i", range(0, 19999))) . '];');
echo count($a), ' ', $a['k19999'], "\n";
function total(...$values) { return array_sum($values); }
echo eval('return total(' . implode(', ', range(1, 20000)) . ');'), "\n";
$v = 14321;
echo eval('return match ($v) { ' . implode(', ', array_map(fn ($i) => "$i => 'arm $i'", range(0, 19999))) . ' };'), "\n";
eval('$s = "' . str_repeat('{$v}-', 5000) . '";');
echo strlen($s), "\n";
--EXPECT--
20000 19999
200010000
arm 14321
30000
