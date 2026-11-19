--TEST--
Destructuring 8,000 nested pairs into 16,000 variables
--FILE--
<?php
$rows = array_map(fn ($i) => [$i, -$i], range(0, 7999));
eval('[' . implode(', ', array_map(fn ($i) => "[\$a$i, \$b$i]", range(0, 7999))) . '] = $rows;');
echo $a0, ' ', $b1, ' ', $a7999, ' ', $b7999, "\n";
--EXPECT--
0 -1 7999 -7999
