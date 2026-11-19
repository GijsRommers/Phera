<?php
// Rule: return.missing; generator_bare_return_good.
function generate(bool $abort): Generator {
    if ($abort) { return; }
    yield 1;
}
