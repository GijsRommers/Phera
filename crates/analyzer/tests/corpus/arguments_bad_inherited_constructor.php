<?php
// Rule: arguments.count / argument.unknown; arguments bad inherited constructor.
class P { function __construct($a) {} } class C extends P {} new C; // error0
