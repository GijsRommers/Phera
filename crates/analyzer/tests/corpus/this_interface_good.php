<?php
// Rule: method.notFound / property.notFound; a narrowed receiver retains its class members.
interface KnowsSecond { function second(); }
class First {
    private int $value = 1;
    function call() {
        if ($this instanceof KnowsSecond) {
            $this->second();
            echo $this->value;
        }
    }
}
