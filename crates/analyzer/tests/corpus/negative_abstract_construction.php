<?php
// Rule: new.abstract; deliberate single error: abstract_construction.
abstract class A {} function f() { return new A; }
