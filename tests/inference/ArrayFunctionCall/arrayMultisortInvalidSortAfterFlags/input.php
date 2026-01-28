<?php
/** @var array<string, array<string>> $test */
array_multisort(
    array_column($test, "s"),
    SORT_NATURAL,
    SORT_DESC,
    $test
);
