<?php
/** @psalm-taint-specialize */
function data(array $data, string $key) {
    return $data[$key];
}

function get(string $key) {
    return data($_GET, $key);
}

function post(string $key) {
    return data($_POST, $key);
}

echo get("x");
/** @psalm-suppress TaintedInput */
echo post("x");
