<?php
/**
 * @param array<string, mixed> $array
 * @return array<string, mixed>
 */
function getArray(array $array): array {
    if (rand(0, 1)) {
        $array["a"] = 2;
    } else {
        $array["b"] = 1;
    }
    return $array;
}
