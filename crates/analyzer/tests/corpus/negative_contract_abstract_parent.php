<?php
// Rule: method.abstract; deliberate single error: contract_abstract_parent.
abstract class A { abstract function f(); } class C extends A {}
