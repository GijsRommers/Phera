<?php
// Rule: members / constants / object contexts; member good constant import.
namespace A { const VALUE = 1; } namespace B { use const A\VALUE as V; echo V; }
