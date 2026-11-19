<?php
// Rule: instanceof.alwaysTrue/alwaysFalse requires a proof from the possible object types.
class CheckAnimal {}
final class CheckDog extends CheckAnimal {}
function checkKind(CheckAnimal $animal): void {
    echo $animal instanceof CheckDog;
}
