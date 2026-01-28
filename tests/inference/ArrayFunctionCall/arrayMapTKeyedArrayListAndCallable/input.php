<?php
/** @param list<int> $list */
function takesList(array $list): void {}

takesList(
    array_map(
        "intval",
        ["1", "2", "3"]
    )
);
