<?php
// Rule: argument.type checks the PHPStan scalar contract even in a non-strict caller.
function takesInt(int $value): int { return $value; }
function takesString(string $value): string { return $value; }
function checkArgumentTypes(string $text): void {
    echo takesInt($text);
    echo takesInt('5');
    echo takesString(5);
}
