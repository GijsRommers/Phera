<?php
// Rule: Boolean operand diagnostics apply to constants in the scopes where they are evaluated.
function checkBoolean(bool $flag, bool $other): void {
    echo $flag && $other;
    echo $flag || $other;
    echo !$flag;
    echo $flag xor $other;
}
