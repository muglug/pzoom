<?php
function getCachedMixed(array $cache, string $locale) : string {
    if (!isset($cache[$locale])) {
        $cache[$locale] = 5;
    }

    /**
     * @psalm-suppress MixedReturnStatement
     */
    return $cache[$locale];
}
