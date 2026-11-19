<?php
// Rule: variable.undefined; or_assignment_good.
function render(bool $flag) {
    if ($flag || !($length = strlen('value'))) {
        return;
    }
    echo $length;
}
