<?php
// Rule: control flow / variable.undefined / return; loop_break_missing_return.
function f($flag): int { // error1
    while (true) {
        if ($flag) { break; }
        return 1;
    }
}
