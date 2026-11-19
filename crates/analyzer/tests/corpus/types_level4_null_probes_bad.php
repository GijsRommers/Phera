<?php
// Rule: nullsafe.neverNull reports access when the receiver cannot be null.
final class CheckNullProbes {
    public int $count = 1;
    public function value(): int { return $this->count; }
}
function checkProbes(CheckNullProbes $known): void {
    echo $known?->value();
    echo $known?->count;
}
