<?php
$arr = [
    "profile" => [
        "foo" => "bar",
    ],
    "groups" => [
        "foo" => "bar",
        "hide"  => rand() % 2 > 0,
    ],
];

foreach ($arr as $item) {
    if (!isset($item["hide"]) || !$item["hide"]) {}
}