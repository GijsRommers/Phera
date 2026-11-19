<?php
// Rule: control flow / variable.undefined / return; loop_break_missing_return.
function f($flag): int { // error0
    while (true) {
        if ($flag) { break; }
        return 1;
    }
}
