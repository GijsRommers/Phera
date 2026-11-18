<?php
// Rule: scope / inheritance; unknown_parent_member.
class C extends Missing { function f() { $this->inherited(); } } // error0
