<?php
// Rule: references / scope / traits; trait_class_overrides_alias.
trait T { function f($x) {} } class C { use T { f as alias; } function alias() {} function g() { $this->alias(); } }
