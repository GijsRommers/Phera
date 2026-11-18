<?php
// Rule: variable.undefined; variable possible foreach.
foreach ([] as $x) {}
echo $x; // error1
