<?php
// Rule: variable.undefined; and_assignment_good.
function lookup($index) { return $index ? $index : null; }
function render(array $items) {
    if (isset($items[0]) && !is_null($value = lookup($items[0])) && $label = strlen((string) $value)) {
        echo $label;
    }
}
