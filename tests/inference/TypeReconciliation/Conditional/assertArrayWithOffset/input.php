<?php
/**
 * @param mixed $decoded
 * @return array{icons:mixed, ...}
 */
function assertArrayWithOffset($decoded): array {
    if (!is_array($decoded)
        || !isset($decoded["icons"])
    ) {
        throw new RuntimeException("Bad");
    }

    return $decoded;
}
