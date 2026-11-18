<?php
// Rule: return.void / return.missing; return good never throw.
function f(): never { throw new Exception; }
