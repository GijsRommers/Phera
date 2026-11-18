<?php
// Rule: method.notFound; deliberate single error: method_parent_forwarded.
class A {} class B extends A { function f() { parent::unavailable(); } }
