<?php
// Rule: members / constants / object contexts; member good this closure.
class C { function f() { return function () { return $this; }; } }
