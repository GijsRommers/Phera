<?php
// Rule: arguments.count; deliberate single error: argument_count_trait_alias.
trait T { static function f($first) {} } class C { use T { f as g; } } C::g();
