<?php
function getPath(): string|object {
    return rand(0, 1) ? "a" : new stdClass();
}

function foo(string $s) : string {
    if (($path = $s) || ($path = getPath())) {
        return $path;
    }

    return "b";
}
