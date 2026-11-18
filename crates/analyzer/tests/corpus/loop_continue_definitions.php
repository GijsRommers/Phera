<?php
// Rule: control flow / variable.undefined / return; loop_continue_definitions.
foreach ([1] as $value) {
    if (rand(0, 1)) { $x = 1; continue; }
    $x = 2;
}
echo $x;
