<?php
// Rule: arguments.count / argument.unknown; arguments bad static many.
class C { static function go() {} } C::go(1); // error0
