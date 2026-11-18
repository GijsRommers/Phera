<?php
// Rule: argument.unknown; deliberate single error: argument_named_function_optional.
function target($first = 0) {} target(unavailable: 1);
