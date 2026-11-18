<?php
// Rule: property.notFound; deliberate single error: property_this_return.
class C { function f() { return $this->unavailable; } }
