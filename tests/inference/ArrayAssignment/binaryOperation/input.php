<?php
$a = array_map(
    function (string $x) {
        return new RuntimeException($x);
    },
    ["c" => ""]
);

$a += ["e" => new RuntimeException()];
