<?php
// Rule: Discarded pure values are reported when the expression was used as a statement.
final class CheckPure {
    public function log(): int { echo 1; return 1; }
}
function checkEffects(string $value, CheckPure $reader): void {
    echo strlen($value);
    $reader->log();
    echo $value;
    $newReader = new CheckPure();
    $newReader->log();
}
