<?php
// Rule: staticProperty.notFound; deliberate single error: property_self_static.
class C { static function f() { return self::$unavailable; } }
