<?php
// Rule: variable.undefined; and_assignment_bad.
function render(bool $flag) {
    $flag && ($length = strlen('value'));
    echo $length;
}
