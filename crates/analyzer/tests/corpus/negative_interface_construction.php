<?php
// Rule: new.interface; deliberate single error: interface_construction.
interface I {} function f() { return new I; }
