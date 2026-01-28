<?php
/** @var array<string, array<string>> $test */
array_multisort(
    array_column($test, "s"),
    $test,
    SORT_NATURAL|SORT_FLAG_CASE,
    SORT_LOCALE_STRING,
);
