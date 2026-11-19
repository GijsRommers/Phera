<?php
// Rule: References require writable arguments, and mutation checks the incoming type.
function incrementValue(int &$value): void { ++$value; }
function checkReferences(): void {
    $value = 1;
    incrementValue($value);
    preg_match('/x/', 'x', $matches);
    echo count($matches);
}
