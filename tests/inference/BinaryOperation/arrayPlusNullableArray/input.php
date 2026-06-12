<?php
class TypeAlias2 {}

/**
 * @param array<string, TypeAlias2> $type_aliases
 */
function mergeAliases(?array $type_aliases): array {
    $local = ['a' => new TypeAlias2()];
    /** @psalm-suppress PossiblyNullOperand */
    return $local + $type_aliases;
}
