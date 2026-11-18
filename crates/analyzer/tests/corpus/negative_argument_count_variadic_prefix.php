<?php
// Rule: arguments.count; deliberate single error: argument_count_variadic_prefix.
function target($first, ...$rest) {} target();
