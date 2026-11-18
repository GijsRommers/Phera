<?php
// Rule: scope / inheritance; trait_abstract_sibling.
trait T { abstract function f(); } trait U { function f() {} } class C { use T, U; }
