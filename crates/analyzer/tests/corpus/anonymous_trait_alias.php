<?php
// Rule: references / scope / traits; anonymous_trait_alias.
trait T { function f($x) {} } new class { use T { f as alias; } function g() { $this->alias(1); } };
