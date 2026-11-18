<?php
// Rule: classConstant.notFound; deliberate single error: constant_this_class.
class C { function f() { return self::UNAVAILABLE; } }
