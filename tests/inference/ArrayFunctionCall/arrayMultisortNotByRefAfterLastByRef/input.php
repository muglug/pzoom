<?php
/** @var array<string, array{id: int, s: int, bar: string}> $test */
array_multisort(
    array_column($test, "s"),
    SORT_DESC,
    $test,
    SORT_ASC,
    array_column($test, "id"),
);
