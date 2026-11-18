<?php
// Rule: members / constants / object contexts; member good this nested.
class C { function f() { return function () { return fn() => $this; }; } }
