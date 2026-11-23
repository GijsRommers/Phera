<?php
// Rule: PHPDoc callable signatures constrain the arguments their callbacks can accept.
/** @param Closure(int): string $formatter */
function formatValue(Closure $formatter): string { return $formatter(1); }
function checkCallables(): void {
    echo formatValue(fn (string $value): string => $value);
}
