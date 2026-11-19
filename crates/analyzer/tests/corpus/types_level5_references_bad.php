<?php
// Rule: References require writable arguments, and mutation checks the incoming type.
function incrementValue(int &$value): void { ++$value; }
function checkReferences(): void {
    incrementValue(1);
    $value = 'wrong';
    incrementValue($value);
}
