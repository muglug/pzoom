<?php
/** @var array<string, array<string>> $test */
array_multisort(
    array_column($test, "s"),
    SORT_DESC,
    SORT_ASC,
    $test
);
