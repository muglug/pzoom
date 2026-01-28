<?php
function getCachedMixed(array $cache, string $locale) : string {
    if (!isset($cache[$locale][$locale])) {
        /**
         * @psalm-suppress MixedArrayAssignment
         */
        $cache[$locale][$locale] = 5;
    }

    /**
     * @psalm-suppress MixedArrayAccess
     * @psalm-suppress MixedReturnStatement
     */
    return $cache[$locale][$locale];
}
