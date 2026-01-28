<?php
$bar = "bar";

$foo = [
    $bar => function (): string {
        return "baz";
    },
];

$foo = array_filter(
    $foo,
    function (string $key): bool {
        return $key === "bar";
    },
    ARRAY_FILTER_USE_KEY
);
