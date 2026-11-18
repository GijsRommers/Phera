<?php
// Rule: references / scope / traits; nested_trait_alias.
trait T { function f($x) {} } $f = function () { class C { use T { f as alias; } function g() { $this->alias(1); } } };
