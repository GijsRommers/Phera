<?php
// Rule: return.void / return.missing; return bad method void.
class C { function f(): void { return 1; } } // error1
