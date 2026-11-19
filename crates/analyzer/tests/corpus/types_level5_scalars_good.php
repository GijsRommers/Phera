<?php
// Rule: argument.type checks the PHPStan scalar contract even in a non-strict caller.
function takesInt(int $value): int { return $value; }
function takesString(string $value): string { return $value; }
function checkArgumentTypes(int $number, string $text): void {
    echo takesInt($number);
    echo takesString($text);
}
