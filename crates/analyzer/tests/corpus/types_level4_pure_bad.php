<?php
// Rule: Discarded pure values are reported when the expression was used as a statement.
final class CheckPure {
    /** @pure */
    public function examine(): int { return 1; }
}
function checkEffects(string $value, CheckPure $reader): void {
    strlen($value);
    $reader->examine();
    $value;
    new CheckPure();
}
