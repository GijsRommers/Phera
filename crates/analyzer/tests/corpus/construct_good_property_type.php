<?php
// Rule: scope / names / signatures; construct good property type.
class C { public DateTime $date; function f() { $this->date->format("Y"); } }
