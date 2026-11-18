<?php
// Rule: staticProperty.notFound; deliberate single error: property_parent_static.
class A {} class B extends A { function f() { return parent::$unavailable; } }
