--TEST--
An elseif ladder runs the first branch that holds, at any length
--FILE--
<?php
function check($label, $result) { echo "check $label\n"; return $result; }
if (check('a', false)) { echo "A\n"; }
elseif (check('b', false)) { echo "B\n"; }
elseif (check('c', true)) { echo "C\n"; }
elseif (check('d', true)) { echo "D\n"; }
else { echo "E\n"; }
if (check('x', false)) { echo "X\n"; } elseif (check('y', false)) { echo "Y\n"; } else { echo "else\n"; }
$v = 19998;
eval('if ($v === -1) { echo "none"; }' . implode('', array_map(fn ($i) => "elseif (\$v === $i) { echo 'branch $i'; }", range(0, 19999))) . ' else { echo "else"; }');
echo "\n";
--EXPECT--
check a
check b
check c
C
check x
check y
else
branch 19998
