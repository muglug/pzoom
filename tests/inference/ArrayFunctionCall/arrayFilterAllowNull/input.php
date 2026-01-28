<?php
function foo() : array {
    return array_filter(
        array_map(
            /** @return null */
            function (int $arg) {
                return null;
            },
            [1, 2, 3]
        )
    );
}
