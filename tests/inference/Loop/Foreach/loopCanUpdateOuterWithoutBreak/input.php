<?php
/**
 * @param array<int> $mappings
 */
function foo(string $id, array $mappings) : void {
    if ($id === "a") {
        foreach ($mappings as $value) {
            $id = $value;
        }
    }

    if (is_int($id)) {}
}
