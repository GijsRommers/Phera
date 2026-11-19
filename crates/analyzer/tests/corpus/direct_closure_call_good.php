<?php
// Rule: variable.undefined; direct_closure_call_good.
class Receiver { function read() {} }
(function () { $this->read(); })->call(new Receiver());
