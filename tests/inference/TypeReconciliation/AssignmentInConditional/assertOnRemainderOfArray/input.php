<?php
/**
 * @psalm-suppress MixedReturnStatement
 */
function foo(string $file_name) : int {
    while ($data = getData()) {
        if (is_numeric($data[0])) {
            for ($i = 1; $i < count($data); $i++) {
                return $data[$i];
            }
        }
    }

    return 5;
}

function getData() : ?array {
    return rand(0, 1) ? ["a", "b", "c"] : null;
}
