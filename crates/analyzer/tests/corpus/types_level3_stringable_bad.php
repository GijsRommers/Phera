<?php
// Rule: a strict return context requires the declared string value.
declare(strict_types=1);
class ValueLabel implements Stringable {
    public function __toString(): string { return 'ready'; }
}
function label(ValueLabel $value): string { return $value; }
