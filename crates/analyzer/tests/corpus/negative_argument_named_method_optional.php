<?php
// Rule: argument.unknown; deliberate single error: argument_named_method_optional.
class C { static function target($first = 0) {} } C::target(unavailable: 1);
