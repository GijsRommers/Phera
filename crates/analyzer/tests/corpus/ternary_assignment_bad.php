<?php
// Rule: variable.undefined; ternary_assignment_bad.
function render(bool $flag) {
    echo $flag || ($length = strlen('value')) ? $length : 0;
}
