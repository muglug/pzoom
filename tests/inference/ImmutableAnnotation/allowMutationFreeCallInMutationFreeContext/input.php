<?php

/**
 * @psalm-mutation-free
 */
function getData(): array {
    /** @var mixed $arr */
    $arr = $GLOBALS["cachedData"] ?? [];

    return is_array($arr) ? $arr : [];
}

/**
 * @psalm-mutation-free
 * @return mixed
 */
function getDataItem(string $key) {
    return getData()[$key] ?? null;
}
