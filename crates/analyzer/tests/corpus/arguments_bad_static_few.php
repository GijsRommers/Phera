<?php
// Rule: arguments.count / argument.unknown; arguments bad static few.
class C { static function go($a) {} } C::go(); // error0
