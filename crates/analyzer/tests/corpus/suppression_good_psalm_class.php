<?php
// Rule: suppression; suppression good psalm class.
/** @psalm-suppress UndefinedMethod */
class C { function f() { $this->missing(); } }
