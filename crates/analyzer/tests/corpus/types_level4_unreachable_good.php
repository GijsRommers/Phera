<?php
// Rule: deadCode.unreachable starts one diagnostic at the first statement after a terminal path.
function checkReachability(bool $flag): int {
    if ($flag) {
        return 1;
    }
    echo 3;
    return 2;
}
