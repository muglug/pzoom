<?php
function a(): array {
    $type_tokens = getArray();

    for ($i = 0, $l = rand(0,100); $i < $l; ++$i) {
        if ($i > 0 && rand(0,1)) {
            continue;
        }

        $type_tokens[$i] = "";

        if ($i > 1) {
            $type_tokens[$i - 2];
        }
    }

    return [];
}

/** @return array<int, string> */
function getArray(): array {
    return [];
}
