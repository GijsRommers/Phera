<?php
// Rule: method.notFound; member bad missing return method.
class C {} function f(): C { return new C; } f()->missing(); // error0
