<?php
/** @psalm-taint-specialize */
function data(array $data, string $key) {
    return $data[$key];
}

/** @psalm-taint-specialize */
function get(string $key) {
    return data($_GET, $key);
}

echo get("x");
