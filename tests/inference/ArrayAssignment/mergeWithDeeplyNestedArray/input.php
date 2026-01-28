<?php
function getTwoPartsLocale(array $cache, string $a, string $b) : string
{
    if (!isset($cache[$b])) {
        $cache[$b] = array();
    }

    if (!isset($cache[$b][$a])) {
        if (rand(0, 1)) {
            /** @psalm-suppress MixedArrayAssignment */
            $cache[$b][$a] = "hello";
        } else {
            /** @psalm-suppress MixedArrayAssignment */
            $cache[$b][$a] = rand(0, 1) ? "string" : null;
        }
    }

    /**
     * @psalm-suppress MixedArrayAccess
     * @psalm-suppress MixedReturnStatement
     */
    return $cache[$b][$a];
}
