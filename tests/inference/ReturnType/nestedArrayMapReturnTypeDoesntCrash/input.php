<?php
function bar(array $a): array {
    return $a;
}

/**
 * @param array[] $x
 *
 * @return array[]
 */
function foo(array $x): array {
    return array_map(
        "array_merge",
        array_map(
            "bar",
            $x
        ),
        $x
    );
}
