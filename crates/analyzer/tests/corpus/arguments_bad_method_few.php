<?php
// Rule: arguments.count / argument.unknown; arguments bad method few.
class C { function go($a) {} function f() { $this->go(); } } // error0
