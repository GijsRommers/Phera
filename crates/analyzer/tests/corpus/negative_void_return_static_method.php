<?php
// Rule: return.void; deliberate single error: void_return_static_method.
class C { static function f(): void { return null; } }
