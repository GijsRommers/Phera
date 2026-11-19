<?php
// Rule: PHPStan accepts an object that implements Stringable in a weak return context.
class ValueLabel implements Stringable {
    public function __toString(): string { return 'ready'; }
}
function label(ValueLabel $value): string { return $value; }
