<?php
// Rule: variable.undefined; deliberate single error: variable_array_value.
function f() { return ["key" => $unavailable]; }
