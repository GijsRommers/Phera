<?php
// Rule: method.notFound; direct_closure_call_bad.
class Receiver { function read() {} }
(function () { $this->reed(); })->call(new Receiver());
