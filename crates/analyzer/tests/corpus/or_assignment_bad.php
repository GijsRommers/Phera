<?php
// Rule: variable.undefined; or_assignment_bad.
function render(bool $flag) {
    if ($flag || $length = strlen('value')) {
        echo $length;
    }
}
