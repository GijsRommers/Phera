<?php
// Rule: arguments.count / argument.unknown; arguments bad method many.
class C { function go() {} function f() { $this->go(1); } } // error0
