<?php
// Rule: arguments.count / argument.unknown; arguments bad method named.
class C { function go($a = 1) {} function f() { $this->go(wrong: 1); } } // error0
