<?php
// Rule: variable.undefined; deliberate single error: variable_interpolation.
function f() { return "value: $unavailable"; }
