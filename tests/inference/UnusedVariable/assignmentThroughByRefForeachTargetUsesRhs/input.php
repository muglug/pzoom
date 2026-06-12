<?php

/**
 * @param array<string, int|string> $types
 * @return array<string, int|string>
 */
function stringifyInts(array $types): array
{
    foreach ($types as &$type) {
        if (is_int($type)) {
            $new = (string) $type;
            $type = $new;
        }
    }
    unset($type);
    return $types;
}
