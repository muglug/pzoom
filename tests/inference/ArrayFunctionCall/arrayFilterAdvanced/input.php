<?php
$f = array_filter(["a" => 5, "b" => 12, "c" => null], function(?int $val, string $key): bool {
    return true;
}, ARRAY_FILTER_USE_BOTH);
$g = array_filter(["a" => 5, "b" => 12, "c" => null], function(string $val): bool {
    return true;
}, ARRAY_FILTER_USE_KEY);

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
