<?php
// Rule: members / constants / object contexts; member good union.
class A {} class B {} function f(A|B $x) { $x->anything(); }
