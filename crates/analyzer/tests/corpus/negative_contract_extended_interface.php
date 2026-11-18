<?php
// Rule: method.abstract; deliberate single error: contract_extended_interface.
interface I { function f(); } interface J extends I {} class C implements J {}
