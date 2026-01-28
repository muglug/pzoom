<?php
/** @var non-empty-array<array{s: int, v: string}> $test */
array_multisort(
    array_column($test, "s"),
    SORT_DESC,
    SORT_NATURAL|SORT_FLAG_CASE,
    $test
);
