<?php
// Rule: scope / inheritance; abstract_late_new.
abstract class C { function f() { return new static; } }
