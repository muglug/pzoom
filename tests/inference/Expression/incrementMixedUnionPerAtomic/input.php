<?php

/**
 * @param array<array-key, string> $rows
 * @return array<int|string, string>
 */
function collectByKey(array $rows): array {
    $properties = [];
    $key = -1;
    foreach ($rows as $name => $row) {
        if (is_string($name) && $name !== '') {
            $key = $name;
        } else {
            /** @psalm-suppress StringIncrement */
            ++$key;
        }
        $properties[$key] = $row;
    }
    return $properties;
}
