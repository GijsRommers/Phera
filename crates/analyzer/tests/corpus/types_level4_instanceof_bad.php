<?php
// Rule: instanceof.alwaysTrue/alwaysFalse requires a proof from the possible object types.
class CheckAnimal {}
final class CheckDog extends CheckAnimal {}
final class CheckBird {}
function checkKind(CheckDog $dog): void {
    echo $dog instanceof CheckAnimal;
    echo $dog instanceof CheckBird;
}
