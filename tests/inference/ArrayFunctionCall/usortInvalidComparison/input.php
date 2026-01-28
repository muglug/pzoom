<?php
$arr = [["one"], ["two"], ["three"]];

usort(
    $arr,
    function (string $a, string $b): int {
        return strcmp($a, $b);
    }
);
