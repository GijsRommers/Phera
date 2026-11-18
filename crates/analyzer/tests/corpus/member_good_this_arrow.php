<?php
// Rule: members / constants / object contexts; member good this arrow.
class C { function f() { return fn() => $this; } }
