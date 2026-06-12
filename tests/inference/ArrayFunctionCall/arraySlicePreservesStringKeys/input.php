<?php

/**
 * @param array<string, object> $extra_types
 * @return array<string, object>
 */
function tailOfMap(array $extra_types): array {
    return array_slice($extra_types, 1);
}

/**
 * @param list<object> $items
 * @return list<object>
 */
function tailOfList(array $items): array {
    return array_slice($items, 1);
}

/**
 * @param array<string, object> $extra_types
 * @return array<string, object>
 */
function tailPreserved(array $extra_types): array {
    return array_slice($extra_types, 1, null, true);
}
