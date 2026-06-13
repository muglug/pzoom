<?php
$arr = [
    "profile" => [
        "foo" => "bar",
    ],
    "groups" => [
        "foo" => "bar",
        "hide"  => rand(0, 5),
    ],
];

foreach ($arr as $item) {
    if (empty($item["hide"]) || $item["hide"] === 3) {}
}
