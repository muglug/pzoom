<?php
/**
 * @param array<int> $mappings
 */
function foo(string $id, array $mappings) : void {
    if ($id === "a") {
        foreach ($mappings as $value) {
            if (rand(0, 1)) {
                $id = $value;
                continue;
            }
        }
    }

    if (is_int($id)) {}
}
