<?php
// Rule: variable.undefined; variable possible foreach list.
foreach ([] as [$x]) {}
echo $x; // error1
