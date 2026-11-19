<?php
// Rule: variable.undefined; while_assignment_good.
function render(array $items) {
    while (isset($items[0]) && ($item = array_shift($items))) {
        echo $item;
    }
}
