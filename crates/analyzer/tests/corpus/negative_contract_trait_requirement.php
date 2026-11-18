<?php
// Rule: method.abstract; deliberate single error: contract_trait_requirement.
trait T { abstract function f(); } class C { use T; }
