<?php
// Rule: arguments.count; unqualified_trait_alias_bad.
trait Imported { function render($a, $b) {} }
class Consumer {
    use Imported { render as importedRender; }
    function render($a) {}
    function run() { $this->importedRender(1); }
}
