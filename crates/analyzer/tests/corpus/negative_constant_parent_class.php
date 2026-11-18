<?php
// Rule: classConstant.notFound; deliberate single error: constant_parent_class.
class A {} class B extends A { function f() { return parent::UNAVAILABLE; } }
