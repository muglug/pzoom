<?php
function a(): ?int {
    /** @var ?int */
    static $foo = 5;

    if (rand(0, 1)) {
        return $foo;
    }

    $foo = null;

    return $foo;
}