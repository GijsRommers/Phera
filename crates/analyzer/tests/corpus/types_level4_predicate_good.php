<?php
// Rule: function.alreadyNarrowedType and function.impossibleType check constant built-in predicates.
function checkKind(int|string $value): void {
    echo is_int($value);
    echo is_string($value);
}
