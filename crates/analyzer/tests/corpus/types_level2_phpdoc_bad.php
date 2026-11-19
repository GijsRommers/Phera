<?php
// Rule: phpDoc.parseError, parameter.notFound and parameter.phpDocType.
/** @param list< $items */
function malformedList(array $items): void {}
/** @param string $wrong */
function wrongParamName($right): void {}
/** @param string $number */
function wrongParamType(int $number): void {}
/** @return string */
function wrongReturnDoc(int $number): int { return $number; }
/** @return array{bad: } */
function malformedReturn(): array { return []; }
