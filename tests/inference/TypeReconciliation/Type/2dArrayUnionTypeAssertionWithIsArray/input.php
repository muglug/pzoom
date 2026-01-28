<?php
/** @return array<array<string>>|null */
function foo() {
    $ids = rand(0, 1) ? [["hello"]] : null;

    if (is_array($ids)) {
        return $ids;
    }

    return null;
}