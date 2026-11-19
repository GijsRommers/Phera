<?php
// Rule: Boolean operand diagnostics apply to constants in the scopes where they are evaluated.
function checkBoolean(bool $flag): void {
    echo $flag && true;
    echo $flag || false;
    echo !false;
    echo $flag xor false;
}
