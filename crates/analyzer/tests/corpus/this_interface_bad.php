<?php
// Rule: method.notFound; a guard does not add an unlisted method.
interface KnowsSecond { function second(); }
class First {
    private int $value = 1;
    function call() {
        if ($this instanceof KnowsSecond) {
            $this->seccnd();
            echo $this->value;
        }
    }
}
