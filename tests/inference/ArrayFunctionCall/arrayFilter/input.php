<?php
$d = array_filter(["a" => rand(0, 10), "b" => rand(0, 10), "c" => null]);
$e = array_filter(
    ["a" => rand(0, 10), "b" => rand(0, 10), "c" => null],
    function(?int $i): bool {
        return true;
    }
);
