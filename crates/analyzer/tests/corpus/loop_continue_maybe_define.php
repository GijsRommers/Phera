<?php
// Rule: control flow / variable.undefined / return; loop_continue_maybe_define.
foreach ([1] as $value) {
    if (rand(0, 1)) { continue; }
    $x = 2;
}
echo $x; // error1
