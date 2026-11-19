<?php
// Rule: variable.undefined; match_assignment_good.
function render(bool $flag) {
    return match (true) {
        $flag && ($label = strlen('a')) > 0 => $label,
        default => 0,
    };
}
