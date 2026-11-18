<?php
// Rule: variable.undefined; variable possible foreach key.
foreach ([] as $x => $v) {}
echo $x; // error1
