<?php
// Rule: variable.undefined; deliberate single error: variable_array_key.
function f() { return [$unavailable => 1]; }
