<?php
/** @psalm-taint-escape ($type is "int" ? "html" : null) */
function cast(mixed $value, string $type): mixed
{
    if ("int" === $type) {
        return (int) $value;
    }
    return (string) $value;
}

/** @psalm-taint-specialize */
function data(array $data, string $key, string $type) {
    return cast($data[$key], $type);
}

// technically a false-positive, but desired behaviour in lieu
// of better information
echo data($_GET, "x", "int");
