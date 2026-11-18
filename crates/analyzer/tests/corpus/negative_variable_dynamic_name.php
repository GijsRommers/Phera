<?php
// Rule: variable.undefined; deliberate single error: variable_dynamic_name.
function f() { return $$unavailable; }
