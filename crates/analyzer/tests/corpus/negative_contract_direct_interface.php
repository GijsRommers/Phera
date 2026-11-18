<?php
// Rule: method.abstract; deliberate single error: contract_direct_interface.
interface I { function f(); } class C implements I {}
