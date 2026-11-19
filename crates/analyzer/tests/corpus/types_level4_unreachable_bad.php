<?php
// Rule: deadCode.unreachable starts one diagnostic at the first statement after a terminal path.
function checkReachability(bool $flag): int {
    if ($flag) {
        return 1;
    } else {
        return 2;
    }
    echo 3;
    echo 4;
}
