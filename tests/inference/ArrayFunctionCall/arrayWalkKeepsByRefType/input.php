<?php
/** @param array<array-key, mixed> $data */
function scrub(array $data): array {
    array_walk_recursive(
        $data,
        function (mixed &$value): void {
            if (is_string($value)) {
                $value = strtoupper($value);
            }
        },
    );
    return $data;
}
