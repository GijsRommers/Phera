<?php
// Rule: scope / inheritance; guard_function_early.
function f() { if (!function_exists("optional_extension")) { return; } optional_extension(); }
