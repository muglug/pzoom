<?php
/** @param list<string> $list */
function takesList(array $list): void {}

takesList(
    array_map(
        function (string $str): string { return $str . "x"; },
        ["foo", "bar", "baz"]
    )
);
