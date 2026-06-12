<?php
$arr = [
    /**
     * @param array{last_access: int} $item
     */
    static function (array $item): int {
        return $item["last_access"];
    },
];

$arr[0](["last_access" => 0]);
