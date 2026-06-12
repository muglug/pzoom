<?php
class UnionM {}

/**
 * @param non-empty-list<non-empty-list<UnionM>> $array_arg_types
 * @return list<UnionM>|null
 */
function zipM(array $array_arg_types): ?array {
    $array_arg_types = array_map(null, ...$array_arg_types);

    if (!$array_arg_types) {
        return null;
    }
    return [];
}
